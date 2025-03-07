#!/bin/sh

set -e

if [ -z $SUDO_USER ] ; then
	echo "Error:  Run this with sudo." 1>&2
	exit 1;
fi

tunname=lightway

ip tuntap add mode tun dev "${tunname}" user "$SUDO_USER"
ip link set dev "${tunname}" mtu 1350
ip link set dev "${tunname}" up
ip addr add 10.125.0.2/16 dev lightway
