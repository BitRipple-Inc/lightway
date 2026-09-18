use std::sync::Arc;

use bytes::BytesMut;
use lightway_app_utils::{
    PacketCodec as LightwayPacketCodec, PacketCodecFactory as LightwayPacketCodecFactory,
};
use lightway_core::{CodecStatus, PacketCodecResult, PacketDecoder, PacketEncoder};
use lt3_plugin::codec::PacketCodecFactory as Lt3PacketCodecFactory;
use lt3_plugin::codec::{
    BitRippleCodecFactory as InnerFactory, CodecStatus as Lt3CodecStatus, NetworkQuality,
    NetworkQualityHandle, NetworkQualitySnapshot, PacketCodec as Lt3PacketCodec, PacketDecoderType,
    PacketEncoderType,
};
use serde::Serialize;

/// Version of the LT3 statistics object emitted through Lightway's opaque codec hook.
const CODEC_STATISTICS_SCHEMA_VERSION: u8 = 1;

pub use lt3_plugin::config::TunnelArgs;

/// Adapts the LT3 codec factory to Lightway's generic packet-codec interface.
pub struct BitRippleCodecFactory {
    inner: InnerFactory,
}

impl BitRippleCodecFactory {
    pub fn new(tunnel_args: TunnelArgs) -> Self {
        Self {
            inner: InnerFactory::new(tunnel_args),
        }
    }
}

struct EncoderWrapper {
    inner: PacketEncoderType,
}

struct DecoderWrapper {
    inner: PacketDecoderType,
    network_quality: NetworkQualityHandle,
}

/// Stable, application-facing LT3 payload carried by Lightway's opaque statistics string.
#[derive(Serialize)]
struct DecoderStatistics {
    schema_version: u8,
    lt3_off: NetworkQuality,
    lt3_on: NetworkQuality,
}

impl From<NetworkQualitySnapshot> for DecoderStatistics {
    fn from(snapshot: NetworkQualitySnapshot) -> Self {
        Self {
            schema_version: CODEC_STATISTICS_SCHEMA_VERSION,
            lt3_off: snapshot.lt3_off,
            lt3_on: snapshot.lt3_on,
        }
    }
}

/// Serializes one simplified LT3 quality snapshot for Lightway's opaque statistics channel.
fn serialize_network_quality(snapshot: NetworkQualitySnapshot) -> Option<String> {
    serde_json::to_string(&DecoderStatistics::from(snapshot)).ok()
}

impl PacketEncoder for EncoderWrapper {
    fn store(&self, data: &mut BytesMut) -> PacketCodecResult<CodecStatus> {
        match self.inner.store(data)? {
            Lt3CodecStatus::PacketAccepted => Ok(CodecStatus::PacketAccepted),
            Lt3CodecStatus::SkipPacket => Ok(CodecStatus::SkipPacket),
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
            Lt3CodecStatus::PacketAccepted => Ok(CodecStatus::PacketAccepted),
            Lt3CodecStatus::SkipPacket => Ok(CodecStatus::SkipPacket),
        }
    }

    fn stats(&self) -> Option<String> {
        serialize_network_quality(self.network_quality.snapshot())
    }
}

impl LightwayPacketCodecFactory for BitRippleCodecFactory {
    fn build(&self) -> LightwayPacketCodec {
        let codec: Lt3PacketCodec = self.inner.build().expect("LT3 codec failed to build");
        LightwayPacketCodec {
            encoder: Arc::new(EncoderWrapper {
                inner: codec.encoder,
            }),
            decoder: Arc::new(DecoderWrapper {
                inner: codec.decoder,
                network_quality: codec.network_quality,
            }),
            encoded_pkt_receiver: codec.encoded_pkt_receiver,
            decoded_pkt_receiver: codec.decoded_pkt_receiver,
        }
    }

    fn get_codec_name(&self) -> String {
        self.inner.get_codec_name()
    }

    fn shutdown(&self) {
        self.inner.shutdown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_quality_uses_the_versioned_lt3_on_off_schema() {
        let snapshot = NetworkQualitySnapshot {
            lt3_off: NetworkQuality::Poor,
            lt3_on: NetworkQuality::Excellent,
        };

        assert_eq!(
            serialize_network_quality(snapshot).as_deref(),
            Some(r#"{"schema_version":1,"lt3_off":"poor","lt3_on":"excellent"}"#)
        );
    }
}
