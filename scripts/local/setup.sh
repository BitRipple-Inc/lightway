#!/bin/sh

# Modified/simplified version of tests/setup.sh

set -e

[ "$(id -u)" -eq 0 ] || echo "Error:  Need root." 1>&2

setup_ns() {
    ns=$1
    tunname=$2
    local_ip=$3
    peer_ip=$4
    network=$5

    # Create namespace
    ip netns add "${ns}"
    ip netns exec "${ns}" ip link set lo up

    # Setup TUN interface
    if [ -n "$tunname" ]; then
        ip netns exec "${ns}" ip tuntap add mode tun dev "${tunname}"
        ip netns exec "${ns}" ip link set dev "${tunname}" mtu 1350
        ip netns exec "${ns}" ip link set dev "${tunname}" up

        if [ -n "$peer_ip" ]; then
            ip netns exec "${ns}" ip addr replace "${local_ip}" peer "${peer_ip}" dev "${tunname}"
        else
            ip netns exec "${ns}" ip addr replace "${local_ip}" dev "${tunname}"
        fi
        if [ -n "$network" ]; then
            if [ -n "$peer_ip" ]; then
                ip netns exec "${ns}" ip route replace "${network}" via "${local_ip}" dev "${tunname}"
            else
                ip netns exec "${ns}" ip route replace "${network}" dev "${tunname}"
            fi
        fi
    fi
}

delete_ns() {
    ns=$1
    ip netns del "${ns}"
}

setup_bridge_interface() {
    intf_name=$1
    ns1=$2
    ip1=$3
    ns2=$4
    ip2=$5

    ip link add "${intf_name}" netns "${ns1}" type veth peer "${intf_name}" netns "${ns2}"

    ip netns exec "${ns1}" ip addr add "${ip1}" dev "${intf_name}"
    ip netns exec "${ns1}" ip link set "${intf_name}" up
    ip netns exec "${ns2}" ip addr add "${ip2}" dev "${intf_name}"
    ip netns exec "${ns2}" ip link set "${intf_name}" up
}

create_setup() {
    setup_ns lightway-server lightway 10.125.0.1 10.125.0.2 10.125.0.0/16
    setup_ns lightway-client lightway 10.125.0.2 10.125.0.1 10.125.0.0/16

    setup_bridge_interface veth-c2s lightway-client 172.16.0.2/12 lightway-server 172.16.0.1/12
}

delete_setup() {
    # Delete namespace along with all tun interfaces
    delete_ns lightway-client
    delete_ns lightway-server
}


COMMAND="${1:-setup}"
case "${COMMAND}" in
    delete)
        echo "Deleting setup..."
        delete_setup
        ;;
    setup)
        echo "Creating setup..."
        create_setup
        ;;
    *)
        echo "Unknown command: $COMMAND"
        echo "Valid commands: setup (default), delete"
        exit 1
        ;;
esac
