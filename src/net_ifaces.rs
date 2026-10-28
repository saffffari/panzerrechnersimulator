//! IPv4 interface enumeration for LAN discovery and the host's "friends join here" address.
//!
//! A Windows laptop routinely carries several IPv4 interfaces at once: the Wi-Fi adapter, the
//! `vEthernet (WSL)` / `vEthernet (Default Switch)` Hyper-V switches, VPN tunnels, VirtualBox or
//! VMware host-only adapters. A limited broadcast (255.255.255.255) leaves on ONE of them, chosen
//! by route metric, which is often a virtual switch; the friends on the Wi-Fi then never see the
//! beacon. Discovery therefore sends the beacon to every up interface's directed broadcast
//! (`192.168.1.255` for `192.168.1.50/24`), and the host screen lists the addresses with the
//! default-route one first and the virtual adapters last.
//!
//! Windows reads `GetAdaptersAddresses` (iphlpapi). Other platforms report only the default-route
//! address (found by a connected UDP socket, which sends nothing) and keep the limited broadcast.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum InterfaceKind {
    WiFi,
    Ethernet,
    /// Up but unclassified (an unknown adapter type that is not obviously virtual).
    Other,
    /// VPN or tunnel: friends on the same Wi-Fi are never behind it.
    Vpn,
    /// Hyper-V/WSL switches, VirtualBox/VMware host-only, Docker, loopback adapters.
    Virtual,
}

impl InterfaceKind {
    pub fn label(self) -> &'static str {
        match self {
            InterfaceKind::WiFi => "Wi-Fi",
            InterfaceKind::Ethernet => "Ethernet",
            InterfaceKind::Other => "network",
            InterfaceKind::Vpn => "VPN",
            InterfaceKind::Virtual => "virtual",
        }
    }

    /// Friends on the same Wi-Fi or wired LAN can reach this interface.
    pub fn physical(self) -> bool {
        matches!(self, InterfaceKind::WiFi | InterfaceKind::Ethernet | InterfaceKind::Other)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ipv4Interface {
    /// Friendly adapter name ("Wi-Fi", "vEthernet (WSL)"); empty when the platform gives none.
    pub name: String,
    pub addr: Ipv4Addr,
    /// On-link prefix length; `None` when unknown (non-Windows fallback).
    pub prefix_len: Option<u8>,
    pub kind: InterfaceKind,
}

impl Ipv4Interface {
    pub fn broadcast(&self) -> Option<Ipv4Addr> {
        directed_broadcast(self.addr, self.prefix_len?)
    }

    /// `169.254.x.x`: DHCP failed on this adapter; nobody else on the Wi-Fi has such an address.
    pub fn link_local(&self) -> bool {
        self.addr.is_link_local()
    }

    /// "192.168.1.50 (Wi-Fi)" for the host screen.
    pub fn describe(&self) -> String {
        if self.name.is_empty() {
            format!("{} ({})", self.addr, self.kind.label())
        } else if self.kind.physical() {
            format!("{} ({})", self.addr, self.name)
        } else {
            format!("{} ({}, {})", self.addr, self.name, self.kind.label())
        }
    }
}

/// Directed broadcast of `addr/prefix_len`. /31 and /32 links have none (RFC 3021).
pub fn directed_broadcast(addr: Ipv4Addr, prefix_len: u8) -> Option<Ipv4Addr> {
    if prefix_len == 0 || prefix_len > 30 {
        return None;
    }
    let mask = u32::MAX << (32 - u32::from(prefix_len));
    Some(Ipv4Addr::from(u32::from(addr) | !mask))
}

/// Name/description markers of adapters a friend on the Wi-Fi can never reach.
pub fn classify(name: &str, description: &str, if_type: Option<u32>) -> InterfaceKind {
    let text = format!("{name} {description}").to_ascii_lowercase();
    const VIRTUAL: [&str; 12] = [
        "vethernet", "hyper-v", "wsl", "virtualbox", "vmware", "virtual", "docker", "npcap",
        "loopback", "host-only", "vbox", "default switch",
    ];
    const VPN: [&str; 9] = [
        "vpn", "tap-", "tap ", "wireguard", "tailscale", "zerotier", "openvpn", "wintun", "anyconnect",
    ];
    if VIRTUAL.iter().any(|marker| text.contains(marker)) {
        return InterfaceKind::Virtual;
    }
    if VPN.iter().any(|marker| text.contains(marker)) {
        return InterfaceKind::Vpn;
    }
    match if_type {
        Some(71) => InterfaceKind::WiFi,                // IF_TYPE_IEEE80211
        Some(6) => InterfaceKind::Ethernet,             // IF_TYPE_ETHERNET_CSMACD
        Some(23 | 108 | 131) => InterfaceKind::Vpn,     // PPP, PPP multilink, tunnel
        Some(24) => InterfaceKind::Virtual,             // software loopback
        Some(53) => InterfaceKind::Virtual,             // IF_TYPE_PROP_VIRTUAL
        _ if text.contains("wi-fi") || text.contains("wireless") || text.contains("wlan") => {
            InterfaceKind::WiFi
        }
        _ if text.contains("ethernet") => InterfaceKind::Ethernet,
        _ => InterfaceKind::Other,
    }
}

/// The local address of the default IPv4 route, if any. A UDP `connect` only selects a route;
/// no datagram leaves the machine.
pub fn default_route_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))).ok()?;
    socket.connect(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 1), 9))).ok()?;
    match socket.local_addr().ok()? {
        SocketAddr::V4(local) if !local.ip().is_unspecified() && !local.ip().is_loopback() => {
            Some(*local.ip())
        }
        _ => None,
    }
}

/// Every up, non-loopback IPv4 interface, best LAN candidate first: the default-route address,
/// then Wi-Fi/Ethernet, then the rest; link-local (169.254) and virtual adapters last.
pub fn ipv4_interfaces() -> Vec<Ipv4Interface> {
    let mut found = platform::enumerate();
    let default = default_route_ipv4();
    if let Some(addr) = default {
        if !found.iter().any(|iface| iface.addr == addr) {
            found.push(Ipv4Interface {
                name: String::new(),
                addr,
                prefix_len: None,
                kind: InterfaceKind::Other,
            });
        }
    }
    sort_for_lan(&mut found, default);
    found
}

pub fn sort_for_lan(interfaces: &mut Vec<Ipv4Interface>, default: Option<Ipv4Addr>) {
    interfaces.retain(|iface| !iface.addr.is_loopback() && !iface.addr.is_unspecified());
    interfaces.sort_by_key(|iface| {
        (
            iface.link_local(),
            !iface.kind.physical(),
            Some(iface.addr) != default,
            iface.kind,
            u32::from(iface.addr),
        )
    });
    interfaces.dedup_by_key(|iface| iface.addr);
}

/// The addresses a friend could type, physical first. Virtual adapters are left out unless they
/// are all there is (a host on a VM-only network still shows something).
pub fn lan_addresses() -> Vec<Ipv4Interface> {
    let all = ipv4_interfaces();
    let usable: Vec<_> = all
        .iter()
        .filter(|iface| iface.kind.physical() && !iface.link_local())
        .cloned()
        .collect();
    if usable.is_empty() { all } else { usable }
}

/// Discovery destinations: every interface's directed broadcast plus the limited broadcast,
/// deduplicated, all on `port`.
pub fn broadcast_targets(interfaces: &[Ipv4Interface], port: u16) -> Vec<SocketAddr> {
    let mut targets: Vec<SocketAddr> = interfaces
        .iter()
        .filter(|iface| !iface.link_local())
        .filter_map(Ipv4Interface::broadcast)
        .map(|broadcast| SocketAddr::V4(SocketAddrV4::new(broadcast, port)))
        .collect();
    targets.push(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::BROADCAST, port)));
    let mut seen = Vec::with_capacity(targets.len());
    targets.retain(|target| {
        let fresh = !seen.contains(target);
        seen.push(*target);
        fresh
    });
    targets
}

#[cfg(windows)]
mod platform {
    use super::{classify, Ipv4Interface};
    use std::net::Ipv4Addr;
    use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, NO_ERROR};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
        GAA_FLAG_SKIP_MULTICAST, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    use windows_sys::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};

    fn wide(text: *const u16) -> String {
        if text.is_null() {
            return String::new();
        }
        // SAFETY: GetAdaptersAddresses returns NUL-terminated UTF-16 strings that live as long as
        // the adapter buffer, which outlives this call.
        unsafe {
            let mut len = 0usize;
            while *text.add(len) != 0 && len < 512 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(text, len))
        }
    }

    pub fn enumerate() -> Vec<Ipv4Interface> {
        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
        // 15 KiB is Microsoft's recommended first try; grow on ERROR_BUFFER_OVERFLOW. The buffer
        // is u64-backed so the adapter structs are suitably aligned.
        let mut size: u32 = 15 * 1024;
        let mut buffer: Vec<u64> = Vec::new();
        let mut result = ERROR_BUFFER_OVERFLOW;
        for _ in 0..4 {
            buffer = vec![0u64; (size as usize).div_ceil(8)];
            // SAFETY: the buffer holds `size` writable, 8-aligned bytes.
            result = unsafe {
                GetAdaptersAddresses(
                    u32::from(AF_INET),
                    flags,
                    std::ptr::null(),
                    buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>(),
                    &mut size,
                )
            };
            if result != ERROR_BUFFER_OVERFLOW {
                break;
            }
        }
        if result != NO_ERROR {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut adapter = buffer.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        // SAFETY: on NO_ERROR the buffer holds a NULL-terminated linked list of adapters whose
        // pointers all point into the same buffer.
        unsafe {
            while !adapter.is_null() {
                let a = &*adapter;
                if a.OperStatus == IfOperStatusUp {
                    let name = wide(a.FriendlyName);
                    let description = wide(a.Description);
                    let kind = classify(&name, &description, Some(a.IfType));
                    let mut unicast = a.FirstUnicastAddress;
                    while !unicast.is_null() {
                        let u = &*unicast;
                        let sockaddr = u.Address.lpSockaddr;
                        if !sockaddr.is_null()
                            && (*sockaddr).sa_family == AF_INET
                            && u.Address.iSockaddrLength as usize >= std::mem::size_of::<SOCKADDR_IN>()
                        {
                            let v4 = &*sockaddr.cast::<SOCKADDR_IN>();
                            // S_addr is in network byte order.
                            let addr = Ipv4Addr::from(u32::from_be(v4.sin_addr.S_un.S_addr));
                            if !addr.is_loopback() {
                                out.push(Ipv4Interface {
                                    name: name.clone(),
                                    addr,
                                    prefix_len: Some(u.OnLinkPrefixLength),
                                    kind,
                                });
                            }
                        }
                        unicast = u.Next;
                    }
                }
                adapter = a.Next;
            }
        }
        out
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn enumerate() -> Vec<super::Ipv4Interface> {
        Vec::new()
    }
}

/// `panzerrechnersystem lan-ifaces`: what discovery will use, for a friend's troubleshooting.
pub fn print_report(game_port: u16, discovery_port: u16) {
    let default = default_route_ipv4();
    let interfaces = ipv4_interfaces();
    println!("IPv4 interfaces (best LAN candidate first):");
    if interfaces.is_empty() {
        println!("  none found: this PC has no IPv4 network. Join the same Wi-Fi as the host.");
    }
    for iface in &interfaces {
        let prefix = iface.prefix_len.map_or("?".to_owned(), |p| p.to_string());
        let broadcast = iface.broadcast().map_or("-".to_owned(), |b| b.to_string());
        let mut notes = Vec::new();
        if Some(iface.addr) == default {
            notes.push("default route");
        }
        if iface.link_local() {
            notes.push("link-local: DHCP failed on this adapter");
        }
        if !iface.kind.physical() {
            notes.push("friends cannot reach this one");
        }
        println!(
            "  {:<15} /{:<2} broadcast {:<15} {:<9} {}{}",
            iface.addr,
            prefix,
            broadcast,
            iface.kind.label(),
            if iface.name.is_empty() { "(unnamed)" } else { &iface.name },
            if notes.is_empty() { String::new() } else { format!("  [{}]", notes.join("; ")) }
        );
    }
    println!("Discovery beacons go to:");
    for target in broadcast_targets(&interfaces, discovery_port) {
        println!("  {target}");
    }
    match lan_addresses().first() {
        Some(best) => println!(
            "Friends type this host address: {}   (game port UDP {game_port}, discovery UDP {discovery_port})",
            best.addr
        ),
        None => println!("No LAN address to offer friends."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(name: &str, addr: [u8; 4], prefix: u8, kind: InterfaceKind) -> Ipv4Interface {
        Ipv4Interface { name: name.into(), addr: Ipv4Addr::from(addr), prefix_len: Some(prefix), kind }
    }

    #[test]
    fn directed_broadcast_follows_the_prefix() {
        let a = Ipv4Addr::new(192, 168, 1, 50);
        assert_eq!(directed_broadcast(a, 24), Some(Ipv4Addr::new(192, 168, 1, 255)));
        assert_eq!(directed_broadcast(Ipv4Addr::new(172, 27, 112, 1), 20), Some(Ipv4Addr::new(172, 27, 127, 255)));
        assert_eq!(directed_broadcast(Ipv4Addr::new(10, 0, 0, 7), 8), Some(Ipv4Addr::new(10, 255, 255, 255)));
        assert_eq!(directed_broadcast(a, 31), None);
        assert_eq!(directed_broadcast(a, 32), None);
        assert_eq!(directed_broadcast(a, 0), None);
    }

    #[test]
    fn windows_virtual_adapters_and_vpns_are_classified() {
        assert_eq!(classify("Wi-Fi", "Intel(R) Wi-Fi 6E AX211 160MHz", Some(71)), InterfaceKind::WiFi);
        assert_eq!(classify("Ethernet", "Realtek PCIe GbE Family Controller", Some(6)), InterfaceKind::Ethernet);
        assert_eq!(classify("vEthernet (WSL (Hyper-V firewall))", "Hyper-V Virtual Ethernet Adapter", Some(6)), InterfaceKind::Virtual);
        assert_eq!(classify("vEthernet (Default Switch)", "Hyper-V Virtual Ethernet Adapter #2", Some(6)), InterfaceKind::Virtual);
        assert_eq!(classify("Ethernet 3", "VirtualBox Host-Only Ethernet Adapter", Some(6)), InterfaceKind::Virtual);
        assert_eq!(classify("Tailscale", "Tailscale Tunnel", Some(53)), InterfaceKind::Vpn);
        assert_eq!(classify("Local Area Connection", "TAP-Windows Adapter V9", Some(6)), InterfaceKind::Vpn);
        assert_eq!(classify("wlan0", "", None), InterfaceKind::WiFi);
    }

    #[test]
    fn the_wifi_address_is_offered_before_virtual_switches() {
        let mut list = vec![
            iface("vEthernet (WSL)", [172, 27, 112, 1], 20, InterfaceKind::Virtual),
            iface("Ethernet 2", [169, 254, 10, 3], 16, InterfaceKind::Ethernet),
            iface("Wi-Fi", [192, 168, 1, 50], 24, InterfaceKind::WiFi),
            iface("Tailscale", [100, 90, 1, 2], 32, InterfaceKind::Vpn),
        ];
        sort_for_lan(&mut list, Some(Ipv4Addr::new(192, 168, 1, 50)));
        let order: Vec<_> = list.iter().map(|i| i.addr.octets()[0]).collect();
        assert_eq!(order, vec![192, 100, 172, 169]);
        assert_eq!(list[0].describe(), "192.168.1.50 (Wi-Fi)");
        assert_eq!(list[2].describe(), "172.27.112.1 (vEthernet (WSL), virtual)");
    }

    #[test]
    fn discovery_reaches_every_subnet_once_plus_the_limited_broadcast() {
        let list = vec![
            iface("Wi-Fi", [192, 168, 1, 50], 24, InterfaceKind::WiFi),
            iface("Wi-Fi", [192, 168, 1, 51], 24, InterfaceKind::WiFi),
            iface("vEthernet (WSL)", [172, 27, 112, 1], 20, InterfaceKind::Virtual),
            iface("Ethernet 2", [169, 254, 10, 3], 16, InterfaceKind::Ethernet),
            iface("Tailscale", [100, 90, 1, 2], 32, InterfaceKind::Vpn),
        ];
        let targets = broadcast_targets(&list, 28_461);
        let shown: Vec<String> = targets.iter().map(ToString::to_string).collect();
        assert_eq!(shown, vec!["192.168.1.255:28461", "172.27.127.255:28461", "255.255.255.255:28461"]);
    }

    #[test]
    fn enumeration_never_returns_loopback() {
        assert!(ipv4_interfaces().iter().all(|iface| !iface.addr.is_loopback()));
    }
}
