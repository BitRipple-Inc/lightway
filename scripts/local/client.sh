#!/bin/sh

set -e

export LW_DANGEROUSLY_DISABLE_PERMISSIONS_CHECKS=1
ip netns exec lightway-client \
	./target/debug/lightway-client \
	--config-file './scripts/local/client_config.yaml' \
	--server '172.16.0.1:27690'
