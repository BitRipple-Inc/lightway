mod args;
mod auth;

use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use clap::CommandFactory;

use metrics_util::debugging::DebuggingRecorder;
use tokio_stream::StreamExt;
use tracing::{error, trace};
use twelf::Layer;

use args::{Config, CodecConfig};
use bitripple_factory_thin_wrapper::BitRippleCodecFactory;
#[cfg(feature = "debug")]
use lightway_app_utils::wolfssl_tracing_callback;
use lightway_app_utils::{PacketCodecFactoryType, TunConfig, Validate, validate_configuration_file_path};
#[cfg(feature = "debug")]
use lightway_core::set_logging_callback;
use lightway_server::*;

async fn metrics_debug() {
  if !tracing::enabled!(tracing::Level::TRACE) {
    return;
  }
  trace!("Logging metrics as trace messages");

  let stats_recorder = DebuggingRecorder::new();

  let snapshotter = stats_recorder.snapshotter();

  if let Err(e) = stats_recorder.install() {
    error!("Error installing stats_recorder: {:?}", e);
    return;
  }

  let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60));
  ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
  let mut ticker = tokio_stream::wrappers::IntervalStream::new(ticker);

  while ticker.next().await.is_some() {
    let snapshot = snapshotter.snapshot();

    for (key, _maybe_units, _maybe_description, value) in snapshot.into_vec() {
      let (_, key) = key.into_parts();
      let (name, labels) = key.into_parts();
      match value {
        metrics_util::debugging::DebugValue::Counter(value) => {
          trace!("metric: {} {labels:?} = Counter({value:?})", name.as_str())
        }
        metrics_util::debugging::DebugValue::Gauge(value) => {
          trace!("metric: {} {labels:?} = Guage({value:?})", name.as_str())
        }
        metrics_util::debugging::DebugValue::Histogram(values) => {
          // TODO: https://docs.rs/average/latest/average/macro.concatenate.html for min/max and avg?

          use average::{Estimate, Max, Mean, Min, concatenate};

          concatenate!(Stats, [Min, min], [Mean, mean], [Max, max]);
          let len = values.len();
          let s: Stats = values.into_iter().map(|f| f.into_inner()).collect();

          trace!(
            "metric: {} {labels:?} = Histogram({} samples min/avg/max {:.2}/{:.2}/{:.2})",
            name.as_str(),
            len,
            s.min(),
            s.mean(),
            s.max(),
          )
        }
      };
    }
  }
}

#[cfg(all(feature = "debug", target_os = "linux"))]
#[allow(unsafe_code)]
async fn log_malloc_info(destination_file_path: PathBuf, divisor: usize, logging_period_in_seconds: u64) {
  use libc;
  use tokio::fs::File;
  use tokio::io::AsyncWriteExt;

  let mut ticker = tokio::time::interval(std::time::Duration::from_secs(logging_period_in_seconds));
  ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
  let mut ticker = tokio_stream::wrappers::IntervalStream::new(ticker);

  let mut destination_file = File::create(destination_file_path)
    .await
    .expect("Failed to open memory debug log destination file");

  destination_file.write_all("timestamp,arena,fordblks,uordblks,hblkhd\n".as_bytes())
    .await
    .expect("Failed to write header to memory debug log");

  while ticker.next().await.is_some() {
    // Excel does not seem to be able to parse the offset, so we use the UTC
    // timestamp and make sure the offset is not printed by converting to the
    // PrimitiveDateTime type.
    let timestamp = time::OffsetDateTime::now_utc();
    let timestamp_without_offset = time::PrimitiveDateTime::new(timestamp.date(), timestamp.time());

    unsafe {
      let mallinfo2 = libc::mallinfo2();
      let message = format!("{},{},{},{},{}\n",
        timestamp_without_offset,
        mallinfo2.arena / divisor,
        mallinfo2.fordblks / divisor,
        mallinfo2.uordblks / divisor,
        mallinfo2.hblkhd / divisor);

      destination_file.write_all(message.as_bytes())
        .await
        .expect("Failed to write data to memory debug log");
    }
  }
}

fn create_codec_factory(codec_config: &CodecConfig) -> PacketCodecFactoryType {
  match codec_config {
    CodecConfig::BitRipple { tunnel_args } => {
      let factory = BitRippleCodecFactory::new(tunnel_args.clone());
      Box::new(factory)
    }
  }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
  let matches = Config::command().get_matches();

  // Fetch the config filepath from CLI and load it as config
  let Some(config_file) = matches.get_one::<PathBuf>("config_file") else {
    return Err(anyhow!("Config file not present"));
  };

  validate_configuration_file_path(config_file, Validate::AllowWorldRead)
    .with_context(|| format!("Invalid configuration file {}", config_file.display()))?;

  let config = Config::with_layers(&[
    Layer::Yaml(config_file.to_owned()),
    Layer::Env(Some(String::from("LW_SERVER_"))),
    Layer::Clap(matches),
  ])?;

  validate_configuration_file_path(&config.server_key, Validate::OwnerOnly)
    .with_context(|| format!("Invalid server key file {}", config.server_key.display()))?;
  validate_configuration_file_path(&config.server_cert, Validate::AllowWorldRead)
    .with_context(|| format!("Invalid server cert file {}", config.server_cert.display()))?;

  if let Some(user_db) = &config.user_db {
    validate_configuration_file_path(user_db, Validate::OwnerOnly)
      .with_context(|| format!("Invalid user db file {}", user_db.display()))?;
  }

  #[cfg(feature = "debug")]
  if config.tls_debug {
    set_logging_callback(Some(wolfssl_tracing_callback));
  }

  let level: tracing::level_filters::LevelFilter = config.log_level.into();
  let filter = tracing_subscriber::EnvFilter::builder()
    .with_default_directive(level.into())
    // https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.Builder.html#method.with_regex
    // recommends to disable REGEX when using envfilter from untrusted sources
    .with_regex(false)
    .with_env_var("LW_SERVER_RUST_LOG")
    .from_env_lossy();
  let fmt = tracing_subscriber::fmt().with_env_filter(filter);

  config.log_format.init_with_env_filter(fmt);

  tokio::spawn(metrics_debug());

  #[cfg(all(feature = "debug", target_os = "linux"))]
  if config.memory_debug_log_enabled {
    tokio::spawn(log_malloc_info(config.memory_debug_log_destination_file,
      config.memory_debug_log_divisor,
      config.memory_debug_log_period_in_seconds));
  }

  std::thread::spawn(move || {
    use parking_lot::deadlock;
    use std::time::Duration;

    loop {
      std::thread::sleep(Duration::from_secs(10));
      let deadlocks = deadlock::check_deadlock();
      if deadlocks.is_empty() {
        continue;
      }

      error!("{} deadlocks detected", deadlocks.len());
      for (i, threads) in deadlocks.iter().enumerate() {
        error!("Deadlock #{}", i);
        for t in threads {
          error!("Thread Id {:#?}", t.thread_id());
          error!("{:#?}", t.backtrace());
        }
      }
    }
  });

  let auth = auth::Auth::new(
    config.user_db.as_ref().map(AsRef::as_ref),
    config.token_rsa_pub_key_pem.as_ref().map(AsRef::as_ref),
  )?;

  let mut tun_config = TunConfig::default();
  if let Some(tun_name) = config.tun_name {
    tun_config.tun_name(tun_name);
  }
  tun_config.up();
  let mode = match config.mode {
    lightway_app_utils::args::ConnectionType::Udp => ServerConnectionMode::Datagram(None),
    lightway_app_utils::args::ConnectionType::Tcp => ServerConnectionMode::Stream(None),
  };

  let inside_pkt_codec = config.inside_pkt_codec.as_ref().map(create_codec_factory);

  let config = ServerConfig {
    mode,
    auth,
    server_cert: config.server_cert,
    server_key: config.server_key,
    tun_config,
    ip_pool: config.ip_pool,
    ip_map: config.ip_map.unwrap_or_default().try_into()?,
    inside_io: None,
    tun_ip: config.tun_ip,
    lightway_server_ip: config.lightway_server_ip,
    lightway_client_ip: config.lightway_client_ip,
    lightway_dns_ip: config.lightway_dns_ip,
    use_dynamic_client_ip: false,
    enable_expresslane: config.enable_expresslane,
    enable_pqc: config.enable_pqc,
    #[cfg(feature = "io-uring")]
    enable_tun_iouring: config.enable_tun_iouring,
    #[cfg(feature = "io-uring")]
    iouring_entry_count: config.iouring_entry_count,
    #[cfg(feature = "io-uring")]
    iouring_sqpoll_idle_time: config.iouring_sqpoll_idle_time.into(),
    key_update_interval: config.key_update_interval.into(),
    inside_plugins: Default::default(),
    outside_plugins: Default::default(),
    inside_pkt_codec,
    bind_address: config.bind_address,
    proxy_protocol: config.proxy_protocol,
    udp_buffer_size: config.udp_buffer_size,
    #[cfg(feature = "debug")]
    randomize_ippool: config.randomize_ippool,
  };

  server(config).await
}
