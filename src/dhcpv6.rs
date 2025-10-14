// SPDX-License-Identifier: Apache-2.0

use std::net::Ipv6Addr;

use mozim::{
    DhcpV6Client, DhcpV6Config, DhcpV6IaType, DhcpV6Lease, DhcpV6Mode,
    DhcpV6State,
};
use nispor::{IfaceConf, IfaceState, IpAddrConf, IpConf, NetConf};

use crate::IFNAME;

pub(crate) const ARG_IS_PD: &str = "pd";
pub(crate) const ARG_IS_TA: &str = "temp";
pub(crate) const ARG_DISPATCH: &str = "dispatch";

const DEFAULT_TIMEOUT: u32 = 480;

pub(crate) async fn run_dhcpv6_cli(matches: &clap::ArgMatches) {
    let iface_name: &String = matches.get_one(IFNAME).unwrap();

    let dispatch_path: String = matches
        .get_one(ARG_DISPATCH)
        .map(|s: &String| s.to_string())
        .unwrap_or_default();

    let mode = if let Some(prefix_len) = matches.get_one(ARG_IS_PD) {
        if dispatch_path.is_empty() {
            eprintln!("Need dispatch script defined for prefix delegation");
            return;
        }
        DhcpV6Mode::PrefixDelegation(*prefix_len)
    } else if matches.get_flag(ARG_IS_TA) {
        if !dispatch_path.is_empty() {
            eprintln!("dispatch script cannot defined without --pd");
            return;
        }
        purge_dhcpv6_addr(iface_name.as_str()).await;
        DhcpV6Mode::TemporaryAddresses
    } else {
        eprintln!("dispatch script cannot defined without --pd");
        purge_dhcpv6_addr(iface_name.as_str()).await;
        DhcpV6Mode::NonTemporaryAddresses
    };

    let mut config = DhcpV6Config::new(iface_name, mode);
    config.set_timeout_sec(DEFAULT_TIMEOUT);
    let mut cli = DhcpV6Client::init(config, None).await.unwrap();
    loop {
        let result = cli.run().await;
        match result {
            Ok(DhcpV6State::Done(lease)) => {
                apply_lease(iface_name, &lease, dispatch_path.as_str()).await;
            }
            Ok(s) => {
                log::info!("DHCPv6 on {iface_name} reach state: {s}");
            }
            Err(e) => {
                log::error!("DHCPv6 on {iface_name} failed: {e}");
                purge_dhcpv6_addr(iface_name).await;
                break;
            }
        }
    }
}

async fn purge_dhcpv6_addr(_iface_name: &str) {
    // TODO
}

async fn apply_lease(
    iface_name: &str,
    lease: &DhcpV6Lease,
    dispatch_path: &str,
) {
    log::trace!("Got DHCPv6 lease {:?}", lease);
    match lease.ia_type {
        Some(DhcpV6IaType::PrefixDelegation) => {
            log::info!(
                "Got DHCPv6 lease of prefix {} length {} for {} seconds",
                lease.address,
                lease.prefix_len,
                lease.valid_time_sec,
            );
            std::process::Command::new(dispatch_path)
                .arg(iface_name)
                .arg(lease.address.to_string())
                .arg(format!("{}", lease.prefix_len))
                .arg(format!("{}", lease.preferred_time_sec))
                .arg(format!("{}", lease.valid_time_sec))
                .status()
                .unwrap_or_else(|_| panic!("Command {} failed to start", dispatch_path));
        }
        Some(DhcpV6IaType::TemporaryAddresses) => {
            log::info!(
                "Got DHCPv6 lease of temporary address {} for {} seconds",
                lease.address,
                lease.valid_time_sec,
            );
            add_ipv6_addr(
                iface_name,
                &lease.address,
                lease.preferred_time_sec,
                lease.valid_time_sec,
            )
            .await;
        }
        Some(DhcpV6IaType::NonTemporaryAddresses) => {
            log::info!(
                "Got DHCPv6 lease of address {} for {} seconds",
                lease.address,
                lease.valid_time_sec,
            );
            add_ipv6_addr(
                iface_name,
                &lease.address,
                lease.preferred_time_sec,
                lease.valid_time_sec,
            )
            .await;
        }
        _ => {
            log::error!(
                "BUG: apply_lease() unsupported lease IA type {:?}: {:?} ",
                lease.ia_type,
                lease
            );
        }
    }
}

async fn add_ipv6_addr(
    iface_name: &str,
    ip: &Ipv6Addr,
    preferred_time_sec: u32,
    valid_time_sec: u32,
) {
    let mut ip_addr_conf = IpAddrConf::default();
    ip_addr_conf.address = ip.to_string();
    ip_addr_conf.prefix_len = 128;
    ip_addr_conf.valid_lft = format!("{}sec", valid_time_sec);
    ip_addr_conf.preferred_lft = format!("{}sec", preferred_time_sec);

    let mut ip_conf = IpConf::default();
    ip_conf.addresses = vec![ip_addr_conf];

    let mut iface_conf = IfaceConf::default();
    iface_conf.name = iface_name.to_string();
    iface_conf.state = IfaceState::Up;
    iface_conf.ipv6 = Some(ip_conf);
    let mut net_conf = NetConf::default();
    net_conf.ifaces = Some(vec![iface_conf]);
    net_conf.apply_async().await.unwrap();
}
