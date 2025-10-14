// SPDX-License-Identifier: Apache-2.0

mod dhcpv6;

use std::str::FromStr;

use mozim::{DhcpV4Client, DhcpV4Config, DhcpV4Lease, DhcpV4State};
use nispor::{
    AddressFamily, IfaceConf, IfaceState, IpAddrConf, IpConf, NetConf,
    NetState, NetStateFilter, NetStateIfaceFilter, NetStateRouteFilter,
    RouteConf, RouteProtocol,
};

use self::dhcpv6::{run_dhcpv6_cli, ARG_DISPATCH, ARG_IS_PD, ARG_IS_TA};

const DEFAULT_METRIC: u32 = 500;
const APP_NAME: &str = "mzc";

const SUBCOMMAND_RUN: &str = "run";
const SUBCOMMAND_RUN6: &str = "run6";
const SUBCOMMAND_PROXY: &str = "proxy";
const SUBCOMMAND_CLEAN: &str = "clean";
const SUBCOMMAND_VERSION: &str = "version";
pub(crate) const IFNAME: &str = "ifname";
const ARG_TIMEOUT: &str = "timeout";
const ARG_MAC: &str = "mac";
const DEFAULT_TIMEOUT_STR: &str = "480";

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let matches = clap::Command::new(APP_NAME)
        .version(clap::crate_version!())
        .author("Gris Ge <fge@redhat.com>")
        .about("Command line of mozim")
        .arg(
            clap::Arg::new(ARG_TIMEOUT)
                .help("Timeout, default to 480 seconds")
                .long(ARG_TIMEOUT)
                .short('t')
                .value_parser(u32::from_str)
                .action(clap::ArgAction::Set)
                .default_value(DEFAULT_TIMEOUT_STR)
                .global(true),
        )
        .subcommand_required(true)
        .subcommand(
            clap::Command::new(SUBCOMMAND_RUN)
                .alias("r")
                .about("Run DHCPv4 Client")
                .arg(
                    clap::Arg::new(IFNAME)
                        .index(1)
                        .help("Interface name")
                        .required(true)
                        .value_parser(
                            clap::builder::NonEmptyStringValueParser::new(),
                        )
                        .action(clap::ArgAction::Set),
                ),
        )
        .subcommand(
            clap::Command::new(SUBCOMMAND_PROXY)
                .alias("p")
                .about("Run DHCP Client proxy")
                .arg(
                    clap::Arg::new(IFNAME)
                        .index(1)
                        .help("Outgoing interface name")
                        .action(clap::ArgAction::Set)
                        .required(true)
                        .value_parser(
                            clap::builder::NonEmptyStringValueParser::new(),
                        ),
                )
                .arg(
                    clap::Arg::new(ARG_MAC)
                        .index(2)
                        .help("MAC address to proxy")
                        .action(clap::ArgAction::Set)
                        .value_parser(
                            clap::builder::NonEmptyStringValueParser::new(),
                        ),
                )
                .arg(
                    clap::Arg::new(ARG_TIMEOUT)
                        .help("Timeout, default to 480 seconds")
                        .long(ARG_TIMEOUT)
                        .short('t')
                        .action(clap::ArgAction::Set)
                        .value_parser(u32::from_str)
                        .default_value(DEFAULT_TIMEOUT_STR),
                ),
        )
        .subcommand(
            clap::Command::new(SUBCOMMAND_CLEAN)
                .alias("c")
                .about("Clean up DHCP IP and routes on specified interface")
                .arg(
                    clap::Arg::new(IFNAME)
                        .index(1)
                        .help("Interface name")
                        .action(clap::ArgAction::Set)
                        .required(true)
                        .value_parser(
                            clap::builder::NonEmptyStringValueParser::new(),
                        ),
                ),
        )
        .subcommand(
            clap::Command::new(SUBCOMMAND_RUN6)
                .alias("r6")
                .about("Run DHCPv6 Client")
                .arg(
                    clap::Arg::new(IFNAME)
                        .index(1)
                        .help("Interface name")
                        .action(clap::ArgAction::Set)
                        .required(true)
                        .value_parser(
                            clap::builder::NonEmptyStringValueParser::new(),
                        ),
                )
                .arg(
                    clap::Arg::new(ARG_IS_PD)
                        .long(ARG_IS_PD)
                        .help("Request DHCPv6 prefix delegation only")
                        .action(clap::ArgAction::Set)
                        .value_parser(u8::from_str),
                )
                .arg(
                    clap::Arg::new(ARG_IS_TA)
                        .long(ARG_IS_TA)
                        .help("Request DHCPv6 temporary addresses only")
                        .action(clap::ArgAction::SetTrue),
                )
                .arg(
                    clap::Arg::new(ARG_DISPATCH)
                        .long(ARG_DISPATCH)
                        .help(
                            "Dispatch script path, only for prefix delegation",
                        )
                        .action(clap::ArgAction::Set),
                ),
        )
        .subcommand(
            clap::Command::new(SUBCOMMAND_VERSION)
                .alias("v")
                .about("Show version"),
        )
        .get_matches();

    init_env_logger();

    let timeout: u32 = *matches.get_one(ARG_TIMEOUT).unwrap();

    if let Some(matches) = matches.subcommand_matches(SUBCOMMAND_RUN) {
        let iface_name: &String = matches.get_one(IFNAME).unwrap();
        run_dhcpv4_cli(iface_name, timeout).await;
    } else if let Some(matches) = matches.subcommand_matches(SUBCOMMAND_PROXY) {
        let iface_name: &String = matches.get_one(IFNAME).unwrap();
        let mac: &String = matches.get_one(ARG_MAC).unwrap();
        proxy(iface_name, mac, timeout).await;
    } else if let Some(matches) = matches.subcommand_matches(SUBCOMMAND_CLEAN) {
        let iface_name: &String = matches.get_one(IFNAME).unwrap();
        purge_dhcp_ip_route(iface_name).await;
    } else if let Some(matches) = matches.subcommand_matches(SUBCOMMAND_RUN6) {
        run_dhcpv6_cli(matches).await;
    } else if matches.subcommand_matches(SUBCOMMAND_VERSION).is_some() {
        println!("{} {}", APP_NAME, clap::crate_version!());
    }
}

async fn run_dhcpv4_cli(iface_name: &str, timeout: u32) {
    purge_dhcp_ip_route(iface_name).await;

    let mut config = DhcpV4Config::new(iface_name);
    config.set_host_name("Gris-Laptop");
    config.use_host_name_as_client_id();
    config.set_timeout_sec(timeout);
    let mut cli = DhcpV4Client::init(config, None).await.unwrap();

    loop {
        let result = cli.run().await;
        match result {
            Ok(DhcpV4State::Done(lease)) => {
                apply_dhcp_ip_route(iface_name, &lease).await;
            }
            Ok(s) => {
                log::info!("DHCPv4 on {iface_name} reach state: {s}");
            }
            Err(e) => {
                log::error!("DHCPv4 on {iface_name} failed: {e}");
                purge_dhcp_ip_route(iface_name).await;
                return;
            }
        }
    }
}

fn new_net_conf_with_ip_conf(iface_name: &str, ip_conf: IpConf) -> NetConf {
    let mut iface_conf = IfaceConf::default();
    iface_conf.name = iface_name.to_string();
    iface_conf.state = IfaceState::Up;
    iface_conf.ipv4 = Some(ip_conf);
    let mut net_conf = NetConf::default();
    net_conf.ifaces = Some(vec![iface_conf]);
    net_conf
}

async fn apply_dhcp_ip_route(iface_name: &str, lease: &DhcpV4Lease) {
    let mut ip_addr_conf = IpAddrConf::default();
    ip_addr_conf.address = lease.yiaddr.to_string();
    ip_addr_conf.prefix_len = get_prefix_len(&lease.subnet_mask);
    ip_addr_conf.valid_lft = format!("{}sec", lease.lease_time_sec);
    ip_addr_conf.preferred_lft = format!("{}sec", lease.lease_time_sec);
    let mut ip_conf = IpConf::default();
    ip_conf.addresses = vec![ip_addr_conf];
    let mut net_conf = new_net_conf_with_ip_conf(iface_name, ip_conf);
    if let Some(gws) = lease.gateways.as_ref() {
        let mut routes = Vec::new();
        for (i, gw) in gws.as_slice().iter().enumerate() {
            routes.push(gen_rt_conf(
                false,
                "0.0.0.0/0",
                iface_name,
                &gw.to_string(),
                Some(DEFAULT_METRIC + i as u32),
            ));
        }
        if !routes.is_empty() {
            net_conf.routes = Some(routes);
        }
    }

    log::debug!("Applying {:?}", net_conf);
    net_conf.apply_async().await.unwrap();
}

fn get_prefix_len(ip: &std::net::Ipv4Addr) -> u8 {
    u32::from_be_bytes(ip.octets()).count_ones() as u8
}

// Remove all dynamic IP and dhcp routes of specified interface
async fn purge_dhcp_ip_route(iface_name: &str) {
    let mut iface_filter = NetStateIfaceFilter::minimum();
    iface_filter.iface_name = Some(iface_name.to_string());
    iface_filter.include_ip_address = true;
    let mut route_filter = NetStateRouteFilter::default();
    route_filter.protocol = Some(RouteProtocol::Dhcp);

    let mut filter = NetStateFilter::minimum();
    filter.iface = Some(iface_filter);
    filter.route = Some(route_filter);

    let state = NetState::retrieve_with_filter_async(&filter).await.unwrap();
    if let Some(ip_info) =
        state.ifaces.get(iface_name).and_then(|i| i.ipv4.as_ref())
    {
        let mut addrs_to_remove = Vec::new();
        for addr in ip_info
            .addresses
            .as_slice()
            .iter()
            .filter(|a| a.valid_lft != "forever")
        {
            let mut addr_conf = IpAddrConf::default();
            addr_conf.remove = true;
            addr_conf.address = addr.address.clone();
            addr_conf.prefix_len = addr.prefix_len;
            addrs_to_remove.push(addr_conf);
        }
        if !addrs_to_remove.is_empty() {
            let mut ip_conf = IpConf::default();
            ip_conf.addresses = addrs_to_remove;
            new_net_conf_with_ip_conf(iface_name, ip_conf)
                .apply_async()
                .await
                .unwrap();
        }
    }
    let mut routes_to_remove = Vec::new();
    for rt in state.routes.as_slice().iter().filter(|rt| {
        rt.oif.as_deref() == Some(iface_name)
            && rt.protocol == RouteProtocol::Dhcp
            && rt.address_family == AddressFamily::IPv4
    }) {
        routes_to_remove.push(gen_rt_conf(
            true,
            rt.dst.as_deref().unwrap_or("0.0.0.0/0"),
            iface_name,
            rt.via
                .as_deref()
                .unwrap_or_else(|| rt.gateway.as_deref().unwrap_or("0.0.0.0")),
            None,
        ));
    }
    let mut net_conf = NetConf::default();
    net_conf.routes = Some(routes_to_remove);
    net_conf.apply_async().await.unwrap();
}

fn gen_rt_conf(
    remove: bool,
    dst: &str,
    oif: &str,
    via: &str,
    metric: Option<u32>,
) -> RouteConf {
    let mut rt = RouteConf::default();
    rt.remove = remove;
    rt.dst = dst.to_string();
    rt.oif = Some(oif.to_string());
    rt.via = Some(via.to_string());
    rt.table = Some(254);
    rt.metric = metric;
    rt.protocol = Some(RouteProtocol::Dhcp);
    rt
}

async fn proxy(iface_name: &str, mac: &str, timeout: u32) {
    let mut config = DhcpV4Config::new_proxy(iface_name, mac).unwrap();
    config.set_timeout_sec(timeout);
    let mut cli = DhcpV4Client::init(config, None).await.unwrap();

    loop {
        let result = cli.run().await;
        match result {
            Ok(DhcpV4State::Done(lease)) => {
                println!("{lease:?}");
            }
            Ok(s) => {
                log::info!(
                    "DHCPv4 proxy on {iface_name} for {mac} reach state: {s}"
                );
            }
            Err(e) => {
                log::error!(
                    "DHCPv4 proxy on {iface_name} for {mac} failed: {e:?}"
                );
                break;
            }
        }
    }
}

fn init_env_logger() {
    env_logger::Builder::new()
        .filter(Some("nispor"), log::LevelFilter::Debug)
        .filter(Some("mozim"), log::LevelFilter::Debug)
        .filter(Some("mzc"), log::LevelFilter::Debug)
        .init();
}
