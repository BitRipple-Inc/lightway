#!/bin/sh

set -e

export LW_DANGEROUSLY_DISABLE_PERMISSIONS_CHECKS=1
./target/debug/lightway-server \
	--config-file './scripts/twomachines/server_config.yaml'
