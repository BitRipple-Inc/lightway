#!/bin/sh

set -e

export LW_DANGEROUSLY_DISABLE_PERMISSIONS_CHECKS=1
ip netns exec lightway-server \
	./target/debug/lightway-server \
	--config-file './scripts/server_config/server_config.yaml'
