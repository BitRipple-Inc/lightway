use std::sync::Arc;
use bytes::BytesMut;
use lightway_app_utils::{PacketCodec as LWPacketCodec, PacketCodecFactory as LWPacketCodecFactory};
use lightway_core::{CodecStatus, PacketCodecResult, PacketDecoder, PacketEncoder};

use lightway_bitripple_plugin::encoder::{
    BitRippleCodecFactory as InnerFactory,
    PacketCodecFactory as BRPacketCodecFactory,
    PacketCodec as BRPacketCodec,
    PacketDecoderType,
    PacketEncoderType,
    TunnelArgs,
    TunnelInserterArgs,
};

pub use lightway_bitripple_plugin::encoder::{TunnelArgs, TunnelInserterArgs};

/// Wrapper that adapts `BitRippleCodecFactory` to the Lightway `PacketCodecFactory` trait.
pub struct BitRippleCodecFactory {
    inner: InnerFactory,
}

impl BitRippleCodecFactory {
    pub fn new(inserter_args: TunnelInserterArgs, tunnel_args: TunnelArgs) -> Self {
        let inner = InnerFactory::new(inserter_args, tunnel_args);
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
        self.inner.store(data)
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
        self.inner.store(data)
    }
}

impl LWPacketCodecFactory for BitRippleCodecFactory {
    fn build(&self) -> LWPacketCodec {
        let codec: BRPacketCodec = self.inner.build();

        LWPacketCodec {
            encoder: Arc::new(EncoderWrapper { inner: codec.encoder }),
            decoder: Arc::new(DecoderWrapper { inner: codec.decoder }),
            encoded_pkt_receiver: codec.encoded_pkt_receiver,
            decoded_pkt_receiver: codec.decoded_pkt_receiver,
        }
    }

    fn get_codec_name(&self) -> String {
        self.inner.get_codec_name()
    }
}
