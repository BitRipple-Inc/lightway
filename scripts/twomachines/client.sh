#!/bin/sh

set -e

export LW_DANGEROUSLY_DISABLE_PERMISSIONS_CHECKS=1
lightway-client \
	--config-file './scripts/twomachines/client_config.yaml' \
	--server '10.0.90.1:27690'
