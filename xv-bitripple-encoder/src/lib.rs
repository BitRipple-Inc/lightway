/*
@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@
===================================== IMPORTS =====================================
@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@
*/
/*
>>>>>>>>>>>>>>>>>>>>>>>>>>>>>> EXTERNAL IMPORTS >>>>>>>>>>>>>>>>>>>>>>>>>>>>>>
*/
use bytes::BytesMut;
use lightway_app_utils::{PacketCodec, PacketCodecFactory};
use lightway_core::{CodecStatus, PacketCodecResult, PacketDecoder, PacketEncoder};
use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::io::AsFd;
use std::os::unix::net::UnixDatagram;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
/*
>>>>>>>>>>>>>>>>>>>>>>>>>>>>>> INTERNAL IMPORTS >>>>>>>>>>>>>>>>>>>>>>>>>>>>>>
*/
use thiserror::Error;
use tokio::sync::mpsc::{self, UnboundedSender};

/*
>>>>>>>>>>>>>>>>>>>>>>>>>>>>>> CUSTOM ERROR DEFINITION >>>>>>>>>>>>>>>>>>>>>>>>>>>>>>
*/
/// Error types from BitRipple Encoder and Decoder
#[derive(Debug, Error)]
pub enum BitRippleError {
    /// Packet size is invalid
    #[error("Invalid packet size")]
    InvalidPacketSize,
}

/*
@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@
================================ CODEC FACTORY CODE ===============================
@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@
*/
/*
>>>>>>>>>>>>>>>>>>>>>>>>>>>>>> ENCODER & DECODER DEFINITIONS >>>>>>>>>>>>>>>>>>>>>>>>>>>>>>
*/
// BitRippleQ ingress packet encoder
struct BitRippleEncoder {
    // Indicates whether the encoder is enabled
    encoding_state: AtomicBool,
    // File descriptor to send unencodeed packets to codec
    inside_fd: UnixDatagram,
    // Control pipe to keep the bitripple/tunnel_inserter process alive
    _control_pipe: std::fs::File,
}

struct BitRippleDecoder {
    outside_fd: UnixDatagram,
}

/*
>>>>>>>>>>>>>>>>>>>>>>>>>>>>>> ENCODER IMPLEMENTATION >>>>>>>>>>>>>>>>>>>>>>>>>>>>>>
*/
impl BitRippleEncoder {
    /// Creates a new BitRippleQ ingress encoder
    pub fn new(inside_fd: UnixDatagram, control_pipe: std::fs::File) -> Self {
        Self {
            encoding_state: AtomicBool::new(false),
            inside_fd,
            _control_pipe: control_pipe,
        }
    }
}

impl PacketEncoder for BitRippleEncoder {
    fn store(&self, data: &mut BytesMut) -> PacketCodecResult<CodecStatus> {
        if !self.encoding_state.load(Ordering::Relaxed) {
            // Skipping packet as encoder is not enabled.
            return Ok(CodecStatus::SkipPacket);
        }
        match self.inside_fd.send(data) {
            Ok(_) => {}
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                println!("drop when sending to tun");
            }
            Err(ref e) => {
                eprintln!("inside_proc send() failed: {:?}", e);
            }
        };
        // Send the packet in socket
        Ok(CodecStatus::PacketAccepted)
    }

    fn get_encoding_state(&self) -> bool {
        self.encoding_state.load(Ordering::Relaxed)
    }

    fn set_encoding_state(&self, enabled: bool) {
        self.encoding_state.store(enabled, Ordering::Relaxed);
    }
}
/*
>>>>>>>>>>>>>>>>>>>>>>>>>>>>>> DECODER IMPLEMENTATION >>>>>>>>>>>>>>>>>>>>>>>>>>>>>>
*/
impl BitRippleDecoder {
    /// Creates a BitRipple encoder adapter
    pub fn new(outside_fd: UnixDatagram) -> Self {
        Self { outside_fd }
    }
}

impl PacketDecoder for BitRippleDecoder {
    fn store(&self, data: &mut BytesMut) -> PacketCodecResult<CodecStatus> {
        let data = std::mem::take(data);
        match self.outside_fd.send(&data) {
            Ok(_) => {}
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                println!("drop when sending to outside");
            }
            Err(ref e) => {
                eprintln!("inside_proc send() failed: {:?}", e);
            }
        };
        Ok(CodecStatus::PacketAccepted)
    }
}

/*
>>>>>>>>>>>>>>>>>>>>>>>>>>>>>> CODEC FACTORY IMPLEMENTATION >>>>>>>>>>>>>>>>>>>>>>>>>>>>>>
*/
/// BitRipple Q Codec Factory
#[derive(Default)]
pub struct BitRippleCodecFactory {
    pub generic_insert_cmd: Vec<String>
}

impl BitRippleCodecFactory {
    // Uses `generic_insert_cmd` to initiate the encoder and decoder
    pub fn new(generic_insert_cmd: Vec<String>) -> Self {
        Self { generic_insert_cmd }
    }
}

impl PacketCodecFactory for BitRippleCodecFactory {
    fn build(&self) -> PacketCodec {
        // Packet will
        let (encoded_packet_sender, encoded_pkt_receiver) = mpsc::unbounded_channel();
        let (decoded_packet_sender, decoded_pkt_receiver) = mpsc::unbounded_channel();
        let proc = make_inside_processor(
            &self.generic_insert_cmd,
            encoded_packet_sender,
            decoded_packet_sender,
        );
        let encoder = Arc::new(BitRippleEncoder::new(proc.inside_fd, proc._control_pipe));
        let decoder = Arc::new(BitRippleDecoder::new(proc.outside_fd));
        PacketCodec {
            encoder,
            decoder,
            encoded_pkt_receiver,
            decoded_pkt_receiver,
        }
    }

    fn get_codec_name(&self) -> String {
        String::from("BitRipple Q Codec")
    }
}

/*
@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@
================================ PROCESS SPAWNING CODE ===============================
@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@
*/
use nix::errno::Errno;
use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::sys::socket::{setsockopt, sockopt};
use nix::unistd::pipe;

/// Set or clear the FD_CLOEXEC flag on a file descriptor
fn set_cloexec(fd: RawFd, enable: bool) -> nix::Result<()> {
    let flags = fcntl(fd, FcntlArg::F_GETFD)?;
    let new_flags = if enable {
        FdFlag::from_bits_truncate(flags) | FdFlag::FD_CLOEXEC
    } else {
        FdFlag::from_bits_truncate(flags) & !FdFlag::FD_CLOEXEC
    };
    fcntl(fd, FcntlArg::F_SETFD(new_flags))?;

    Ok(())
}

/*
>>>>>>>>>>>>>>>>>>>>>>>>>>>>>> REMOTE ENDPOINTS >>>>>>>>>>>>>>>>>>>>>>>>>>>>>>
*/
// These are the remotes
struct GenericInsideProcessor {
    /// << FOR DECODED PACKETS >>: ---- File descriptor for inside data (to/from tun interface)
    inside_fd: UnixDatagram,
    /// << FOR ENCODED PACKETS >>: ---- File descriptor for outside data (to/from tunnel socket)
    outside_fd: UnixDatagram,
    /// Control pipe, write end.
    _control_pipe: std::fs::File,
}

fn make_inside_processor(
    generic_insert_cmd: &[String],
    encoded_packet_sender: UnboundedSender<BytesMut>,
    decoded_packet_sender: UnboundedSender<BytesMut>,
) -> GenericInsideProcessor {
    // Creating control pipe. We will inherit the read end to subprocesses, but not the write end (write is for caller)
    let (pipe_read, pipe_write) = pipe().unwrap();
    set_cloexec(pipe_read.as_raw_fd(), false).unwrap();
    set_cloexec(pipe_write.as_raw_fd(), true).unwrap();
    // Create the sockets. `other`'s ends of pipe will be inherited by subprocesses, but not the `own`. The `own`s will be send to caller
    let (inside_fd_own, inside_fd_other) = UnixDatagram::pair().unwrap();
    let (outside_fd_own, outside_fd_other) = UnixDatagram::pair().unwrap();
    set_cloexec(inside_fd_other.as_raw_fd(), false).unwrap();
    set_cloexec(outside_fd_other.as_raw_fd(), false).unwrap();
    for fd in [ // Set buffer size
        &inside_fd_own,
        &inside_fd_other,
        &outside_fd_own,
        &outside_fd_other,
    ] {
        setsockopt(fd, sockopt::RcvBuf, &2000000).expect("Can't set SO_RCVBUF");
        setsockopt(fd, sockopt::SndBuf, &2000000).expect("Can't set SO_SNDBUF");
    }
    // Get the command to run, substituting the current FD/control pipe values.
    let pipe_read_str = format!("{}", pipe_read.as_raw_fd());
    let inside_fd_other_str = inside_fd_other.as_raw_fd().to_string();
    let outside_fd_other_str = outside_fd_other.as_raw_fd().to_string();
    let cmd_interp: Vec<String> = generic_insert_cmd
        .iter()
        .map(|s| {
            s.replace("{control}", &pipe_read_str)
                .replace("{inside}", &inside_fd_other_str)
                .replace("{outside}", &outside_fd_other_str)
        })
        .collect();
    // Start the processor as a child process. Here the `other`'s pipe ends and read control pipe will be inherited.
    let mut child_process = std::process::Command::new(&cmd_interp[0])
        .args(&cmd_interp[1..])
        .spawn()
        .expect("Could not start process");
    // We can close in this parent process the FDs we passed on to the child, giving it exclusive access.
    drop(pipe_read);
    drop(inside_fd_other);
    drop(outside_fd_other);
    // This is helpful to avoid possible dead locks. Pipe end returns immediately when nothing to read
    outside_fd_own.set_nonblocking(true).unwrap();
    inside_fd_own.set_nonblocking(true).unwrap();
    // Start processing threads. These are workers that receive data.
    let outside_fd2 = outside_fd_own.try_clone().unwrap();
    std::thread::spawn(move || {
        bitripple_packet_receive_worker(encoded_packet_sender, outside_fd2, false)
    });
    let inside_fd2 = inside_fd_own.try_clone().unwrap();
    std::thread::spawn(move || {
        bitripple_packet_receive_worker(decoded_packet_sender, inside_fd2, true)
    });
    std::thread::spawn(move || {
        child_process.wait()
    });
    GenericInsideProcessor {
        inside_fd: inside_fd_own,
        outside_fd: outside_fd_own,
        _control_pipe: std::fs::File::from(pipe_write),
    }
}

fn bitripple_packet_receive_worker(
    pkt_channel: mpsc::UnboundedSender<BytesMut>,
    fd: UnixDatagram,
    inside: bool,
) {
    let mut poll_fds = [PollFd::new(fd.as_fd(), PollFlags::POLLIN)];
    loop {
        let mut buf = BytesMut::with_capacity(4096);
        buf.resize(buf.capacity(), 0); // XXX there must be a better way.
        match poll(&mut poll_fds, PollTimeout::NONE) {
            Ok(_) => {
                if let Some(revents) = poll_fds[0].revents() {
                    if revents.intersects(PollFlags::POLLHUP | PollFlags::POLLERR) {
                        // Hang up, terminate.
                        return;
                    } else if revents.intersects(PollFlags::POLLIN) {
                        match fd.recv(&mut buf) {
                            Ok(sz) => {
                                buf.truncate(sz);
                                println!(
                                    "Packet to be sent has len {}, inside?: {}",
                                    buf.len(),
                                    inside
                                );
                                match pkt_channel.send(buf) {
                                    Ok(()) => {
                                        // Success. TODO: Log and find Axl logs
                                    }
                                    Err(_pkt) => {
                                        // Channel closed means the encoder has been dropped.
                                        // Terminate.
                                        return;
                                    }
                                }
                            }
                            Err(e)
                                if e.kind() == io::ErrorKind::WouldBlock
                                    || e.kind() == io::ErrorKind::Interrupted =>
                            {
                                // Not a fatal error, retry.
                                continue;
                            }
                            Err(e) => {
                                eprintln!("Unhandled recv() error: {:?}", e);
                                return;
                            }
                        }
                    }
                }
            }
            Err(Errno::EINTR) => {}
            Err(e) => {
                eprintln!("Poll failed: {}", e);
                return;
            }
        }
    }
}
