use std::sync::Arc;

use bytes::BytesMut;
use lightway_app_utils::{
    PacketCodec as LightwayPacketCodec, PacketCodecFactory as LightwayPacketCodecFactory,
};
use lightway_core::{CodecStatus, PacketCodecResult, PacketDecoder, PacketEncoder};
use lt3_plugin::codec::PacketCodecFactory as Lt3PacketCodecFactory;
use lt3_plugin::codec::{
    BitRippleCodecFactory as InnerFactory, CodecStatus as Lt3CodecStatus,
    PacketCodec as Lt3PacketCodec, PacketDecoderType, PacketEncoderType, ReceiverMetricsHandle,
    ReceiverMetricsSnapshot,
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
    receiver_metrics: ReceiverMetricsHandle,
}

/// Stable, application-facing LT3 payload carried by Lightway's opaque statistics string.
#[derive(Serialize)]
struct DecoderStatistics {
    schema_version: u8,
    receiver_side: ReceiverSideStatistics,
}

/// Receiver-owned statistics. Additional metric families can be added here in a later schema.
#[derive(Serialize)]
struct ReceiverSideStatistics {
    object_recovered: ObjectRecoveryStatistics,
}

/// Connection-cumulative Axl object outcomes exposed to Lightway applications.
#[derive(Serialize)]
struct ObjectRecoveryStatistics {
    native: u64,
    recovered: u64,
    unrecovered: u64,
}

impl From<ReceiverMetricsSnapshot> for DecoderStatistics {
    fn from(snapshot: ReceiverMetricsSnapshot) -> Self {
        let counters = snapshot.object_counters;
        Self {
            schema_version: CODEC_STATISTICS_SCHEMA_VERSION,
            receiver_side: ReceiverSideStatistics {
                object_recovered: ObjectRecoveryStatistics {
                    native: counters.native,
                    recovered: counters.recovered,
                    unrecovered: counters.unrecovered,
                },
            },
        }
    }
}

/// Serializes one simplified receiver snapshot for Lightway's opaque statistics channel.
fn serialize_receiver_metrics(snapshot: ReceiverMetricsSnapshot) -> Option<String> {
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
        self.receiver_metrics
            .snapshot()
            .and_then(serialize_receiver_metrics)
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
                receiver_metrics: codec.receiver_metrics,
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
    fn receiver_metrics_use_the_versioned_nested_counter_schema() {
        let snapshot = ReceiverMetricsSnapshot {
            object_counters: lt3_plugin::codec::ReceiverObjectCounters {
                native: 123,
                recovered: 4,
                unrecovered: 1,
            },
        };

        assert_eq!(
            serialize_receiver_metrics(snapshot).as_deref(),
            Some(
                r#"{"schema_version":1,"receiver_side":{"object_recovered":{"native":123,"recovered":4,"unrecovered":1}}}"#
            )
        );
    }

    #[test]
    fn decoder_statistics_are_absent_before_the_lazy_axl_graph_is_ready() {
        let factory = BitRippleCodecFactory::new(TunnelArgs::default());
        let codec = LightwayPacketCodecFactory::build(&factory);

        assert_eq!(codec.decoder.stats(), None);
    }
}
