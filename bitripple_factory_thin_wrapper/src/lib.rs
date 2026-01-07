use bytes::BytesMut;
use lightway_app_utils::{
  PacketCodec as LWPacketCodec, PacketCodecFactory as LWPacketCodecFactory,
};
pub use lightway_bitripple_plugin::config::TunnelArgs;
use lightway_bitripple_plugin::codec::PacketCodecFactory as BrPacketCodecFactory;
use lightway_bitripple_plugin::codec::{
  BitRippleCodecFactory as InnerFactory, CodecStatus as BrCodecStatus,
  PacketCodec as BRPacketCodec, PacketDecoderType, PacketEncoderType,
};
use lightway_core::{CodecStatus, PacketCodecResult, PacketDecoder, PacketEncoder};
use std::sync::Arc;

/// Wrapper that adapts `BitRippleCodecFactory` to the Lightway `PacketCodecFactory` trait.
pub struct BitRippleCodecFactory {
  inner: InnerFactory,
}

impl BitRippleCodecFactory {
  pub fn new(tunnel_args: TunnelArgs) -> Self {
    let inner = InnerFactory::new(tunnel_args);
    Self { inner }
  }
}

struct EncoderWrapper {
  inner: PacketEncoderType,
}

struct DecoderWrapper {
  inner: PacketDecoderType,
}

impl PacketEncoder for EncoderWrapper {
  fn store(&self, data: &mut BytesMut) -> PacketCodecResult<CodecStatus> {
    match self.inner.store(data)? {
      BrCodecStatus::PacketAccepted => Ok(CodecStatus::PacketAccepted),
      BrCodecStatus::SkipPacket => Ok(CodecStatus::SkipPacket),
    }
  }

  fn get_encoding_state(&self) -> bool {
    self.inner.get_encoding_state()
  }

  fn set_encoding_state(&self, enabled: bool) {
    self.inner.set_encoding_state(enabled)
  }
}

impl PacketDecoder for DecoderWrapper {
  fn store(&self, data: &mut BytesMut) -> PacketCodecResult<CodecStatus> {
    match self.inner.store(data)? {
      BrCodecStatus::PacketAccepted => Ok(CodecStatus::PacketAccepted),
      BrCodecStatus::SkipPacket => Ok(CodecStatus::SkipPacket),
    }
  }
}

impl LWPacketCodecFactory for BitRippleCodecFactory {
  fn build(&self) -> LWPacketCodec {
    let codec: BRPacketCodec = self.inner.build().expect("BitRipple Codec Failed to Build");
    LWPacketCodec {
      encoder: Arc::new(EncoderWrapper {
        inner: codec.encoder,
      }),
      decoder: Arc::new(DecoderWrapper {
        inner: codec.decoder,
      }),
      encoded_pkt_receiver: codec.encoded_pkt_receiver,
      decoded_pkt_receiver: codec.decoded_pkt_receiver,
    }
  }
  fn get_codec_name(&self) -> String {
    self.inner.get_codec_name()
  }
}
