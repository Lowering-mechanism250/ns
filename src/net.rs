use anyhow::Result;
use procfs::net::TcpState;
use std::collections::HashMap;
use std::fs;
use std::time::Instant;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct InterfaceStats {
    pub name: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_rate: f64,
    pub tx_rate: f64,
    pub rx_history: Vec<f64>,
    pub tx_history: Vec<f64>,
    pub last_update: Instant,
}

impl InterfaceStats {
    pub fn new(name: String, rx_bytes: u64, tx_bytes: u64) -> Self {
        Self {
            name,
            rx_bytes,
            tx_bytes,
            rx_rate: 0.0,
            tx_rate: 0.0,
            rx_history: vec![0.0; 60],
            tx_history: vec![0.0; 60],
            last_update: Instant::now(),
        }
    }

    pub fn update(&mut self, rx_bytes: u64, tx_bytes: u64) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        if elapsed > 0.0 {
            self.rx_rate = (rx_bytes.saturating_sub(self.rx_bytes) as f64) / elapsed;
            self.tx_rate = (tx_bytes.saturating_sub(self.tx_bytes) as f64) / elapsed;
        }
        self.rx_bytes = rx_bytes;
        self.tx_bytes = tx_bytes;
        self.last_update = now;

        self.rx_history.push(self.rx_rate);
        if self.rx_history.len() > 60 {
            self.rx_history.remove(0);
        }
        self.tx_history.push(self.tx_rate);
        if self.tx_history.len() > 60 {
            self.tx_history.remove(0);
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct OpenPort {
    pub port: u16,
    pub protocol: &'static str,
    pub interface: String,
    pub local_addr: String,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub user: Option<String>,
    pub inode: u64,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Connection {
    pub remote_addr: String,
    pub remote_port: u16,
    pub local_port: u16,
    pub protocol: &'static str,
    pub interface: String,
    pub bytes_per_sec: f64,
    pub connections: u32,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub inode: u64,
    /// true = outgoing (we initiated), false = incoming (remote initiated)
    pub is_outgoing: bool,
}

pub struct NetCollector {
    pub interfaces: HashMap<String, InterfaceStats>,
    /// Map from IP address (as string) -> interface name, built from /proc/net/fib_trie + if_inet6
    iface_ips: HashMap<String, String>,
    inode_to_pid: HashMap<u64, u32>,
    pid_to_name: HashMap<u32, String>,
    pid_to_user: HashMap<u32, String>,
    connection_bytes: HashMap<String, (u64, Instant)>,
}

impl NetCollector {
    pub fn new() -> Self {
        Self {
            interfaces: HashMap::new(),
            iface_ips: HashMap::new(),
            inode_to_pid: HashMap::new(),
            pid_to_name: HashMap::new(),
            pid_to_user: HashMap::new(),
            connection_bytes: HashMap::new(),
        }
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.refresh_interfaces()?;
        self.refresh_iface_ips();
        self.refresh_process_map()?;
        Ok(())
    }

    fn refresh_interfaces(&mut self) -> Result<()> {
        let content = fs::read_to_string("/proc/net/dev")?;
        let mut total_rx: u64 = 0;
        let mut total_tx: u64 = 0;

        for line in content.lines().skip(2) {
            let line = line.trim();
            let colon = match line.find(':') {
                Some(c) => c,
                None => continue,
            };
            let name = line[..colon].trim().to_string();
            let parts: Vec<&str> = line[colon + 1..].split_whitespace().collect();
            if parts.len() < 9 {
                continue;
            }
            let rx_bytes: u64 = parts[0].parse().unwrap_or(0);
            let tx_bytes: u64 = parts[8].parse().unwrap_or(0);

            total_rx += rx_bytes;
            total_tx += tx_bytes;

            if let Some(iface) = self.interfaces.get_mut(&name) {
                iface.update(rx_bytes, tx_bytes);
            } else {
                self.interfaces
                    .insert(name.clone(), InterfaceStats::new(name, rx_bytes, tx_bytes));
            }
        }

        if let Some(all) = self.interfaces.get_mut("all") {
            all.update(total_rx, total_tx);
        } else {
            self.interfaces.insert(
                "all".to_string(),
                InterfaceStats::new("all".to_string(), total_rx, total_tx),
            );
        }

        Ok(())
    }

    /// Build a map of IP -> interface name by reading /proc/net/fib_trie (IPv4)
    /// and /proc/net/if_inet6 (IPv6).
    fn refresh_iface_ips(&mut self) {
        self.iface_ips.clear();

        // IPv4: parse /proc/net/fib_trie
        // We scan for LOCAL host entries, which follow the pattern:
        //   <iface>\n  ... \n  LOCAL <ip>
        // Simpler: use /proc/net/if_inet6 for IPv6, and /proc/net/arp + netlink
        // for IPv4. The most portable approach without extra crates is to read
        // /proc/net/fib_trie and match LIBCSS LOCAL entries.
        if let Ok(content) = fs::read_to_string("/proc/net/fib_trie") {
            let mut current_ip: Option<String> = None;
            for line in content.lines() {
                let trimmed = line.trim();
                // Lines like "  192.168.1.5  LOCAL" carry the IP
                if trimmed.ends_with("LOCAL") {
                    let ip_str = trimmed.split_whitespace().next().unwrap_or("");
                    current_ip = Some(ip_str.to_string());
                } else if trimmed.starts_with("|--")
                    || (!trimmed.starts_with('/') && trimmed.contains('.'))
                {
                    // potential IP line without LOCAL tag — reset
                    current_ip = None;
                }
                // Look for interface names that follow a LOCAL host
                // Actually fib_trie has a different structure — use fib_triestat / route
                // Better: parse /proc/net/dev then /proc/net/fib_trie carefully below
                let _ = current_ip.as_ref(); // suppress unused warning
            }
            // Reset since above naive parse doesn't work well; use correct method below
            self.iface_ips.clear();
            self.parse_fib_trie(&content);
        }

        // IPv6: /proc/net/if_inet6 format: <addr_hex> <dev_num> <prefix_len> <scope> <flags> <devname>
        if let Ok(content) = fs::read_to_string("/proc/net/if_inet6") {
            for line in content.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 6 {
                    continue;
                }
                let hex = parts[0];
                let iface = parts[5].to_string();
                // Convert 32-hex-char to IPv6
                if let Some(ip) = hex_to_ipv6(hex) {
                    self.iface_ips.insert(ip, iface);
                }
            }
        }
    }

    /// Parse /proc/net/fib_trie to extract LOCAL IPs and their interface names.
    /// The file structure has sections like:
    ///   Main:
    ///     +-- ...
    ///        |-- <prefix>/<len>
    ///           /32 host LOCAL
    ///              <ip>
    /// We use a simpler approach: cross-reference with /proc/net/fib_triestat
    /// or just parse ip addr from /proc/net/if_inet and arp.
    ///
    /// Most reliable without netlink: read each interface's addr from sysfs.
    fn parse_fib_trie(&mut self, _content: &str) {
        // Sysfs approach: iterate /sys/class/net/<iface>/address (MAC only).
        // For IPs, read /proc/net/fib_trie properly.
        //
        // Correct fib_trie parsing: lines are indented blocks. LOCAL entries look like:
        //    32 host LOCAL
        // And the IP comes from the parent block header like:
        //    |-- 192.168.1.5
        //       /32 host LOCAL
        //
        // Read sysfs /sys/class/net for interface list, then for each interface
        // check /proc/net/fib_trie. But the simplest correct approach is to use
        // /proc/net/tcp + local IP matching.
        //
        // Instead, enumerate /sys/class/net/<iface>/ and read each interface's
        // IPv4 address via ioctl simulation — too complex without libc directly.
        //
        // Best portable approach: parse /proc/net/dev to get iface names, then
        // read /proc/net/fib_trie and use the pattern recognition below.

        // Get all known interface names (excluding "all")
        let iface_names: Vec<String> = self
            .interfaces
            .keys()
            .filter(|n| n.as_str() != "all")
            .cloned()
            .collect();

        for iface in &iface_names {
            // Try reading IPs from sysfs: /sys/class/net/<iface>/operstate exists,
            // but IP addresses aren't directly in sysfs files.
            // Fallback: we read /proc/net/fib_trie and look for the correct structure.
            let _ = iface;
        }

        // Correct fib_trie parser
        let content = match fs::read_to_string("/proc/net/fib_trie") {
            Ok(c) => c,
            Err(_) => return,
        };

        // We'll also read /proc/net/route to map interface -> gateway/subnet
        // and then match IPs. Actually the simplest cross-ref:
        // /proc/net/fib_trie lines with "LOCAL" preceded by an IP in the hierarchy.
        let mut stack: Vec<String> = Vec::new();
        let mut last_ip: Option<String> = None;

        for line in content.lines() {
            let trimmed = line.trim_start();
            let depth = line.len() - trimmed.len();

            // Truncate stack to current depth
            // Each depth level is 3 chars of indentation ("|-- " prefix etc)
            let level = depth / 3;
            if stack.len() > level {
                stack.truncate(level);
            }

            // Detect IP address lines (contain dots and look like an addr)
            // They appear as:  |-- <ip>  or   +-- <ip>
            if let Some(ip_part) = trimmed
                .strip_prefix("|-- ")
                .or_else(|| trimmed.strip_prefix("+-- "))
            {
                // might be an IP or a prefix like 192.168.0.0/16
                let candidate = ip_part.split('/').next().unwrap_or("");
                if is_ipv4(candidate) {
                    last_ip = Some(candidate.to_string());
                }
            }

            // Detect LOCAL host entries
            if trimmed.contains("LOCAL") && trimmed.contains("host") {
                if let Some(ref ip) = last_ip {
                    // Now we need the interface name. Parse /proc/net/route to find it.
                    // We'll store the IP for now and resolve later.
                    self.iface_ips.insert(ip.clone(), String::new()); // placeholder
                }
            }
        }

        // Cross-reference with /proc/net/route to assign interface names
        if let Ok(route_content) = fs::read_to_string("/proc/net/route") {
            for line in route_content.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 8 {
                    continue;
                }
                let iface = parts[0].to_string();
                // destination is parts[1] (hex), gateway parts[2], mask parts[7]
                // source/local IP: not directly here.
                // At minimum record interface exists
                let _ = iface;
            }
        }

        // Better approach: use /proc/net/if_inet6 for v6 (done above),
        // and for v4, iterate /sys/class/net/<iface>/ subdirs with `ip_address` files if present.
        // Since there's no reliable file without ioctl, scan tcp entries to resolve:
        // For each LOCAL address in tcp that's not 0.0.0.0, we know it's bound to some iface.
        // We'll resolve actual iface<->ip mapping at connection-lookup time using the route table.

        // Final approach: read /proc/net/fib_trie LOCAL entries and pair with route table
        self.iface_ips.clear();
        self.build_ip_iface_map();
    }

    /// Build IP->iface map using /proc/net/route (for gateway/src routing hints)
    /// combined with scanning /sys/class/net/<iface>/...
    /// Most reliable without external crates: parse /proc/net/fib_trie correctly.
    fn build_ip_iface_map(&mut self) {
        // Read fib_trie and pair LOCAL IPs with their enclosing interface context.
        // The file has two sections: "Main:" and "Local:". We use Local: section
        // which lists all LOCAL addresses. We then cross with /proc/net/route for iface.
        let trie = match fs::read_to_string("/proc/net/fib_trie") {
            Ok(c) => c,
            Err(_) => return,
        };

        let mut local_ips: Vec<String> = Vec::new();
        let mut prev_ip: Option<String> = None;

        for line in trie.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed
                .strip_prefix("|-- ")
                .or_else(|| trimmed.strip_prefix("+-- "))
            {
                let candidate = rest.split('/').next().unwrap_or("");
                if is_ipv4(candidate) {
                    prev_ip = Some(candidate.to_string());
                } else {
                    prev_ip = None;
                }
            } else if trimmed.contains("LOCAL") && trimmed.contains("host") {
                if let Some(ref ip) = prev_ip {
                    if ip != "0.0.0.0" {
                        local_ips.push(ip.clone());
                    }
                }
            }
        }

        // Now use /proc/net/route to find which iface owns each /32 subnet
        // Route table format: Iface Dest Gateway Flags RefCnt Use Metric Mask ...
        // Match: if Dest == ip (in hex) with Mask == FFFFFFFF, that's a host route.
        // More commonly we match Dest network against the IP.
        // Simplest: for each local_ip, find route with Dest in the same subnet.
        let route = match fs::read_to_string("/proc/net/route") {
            Ok(c) => c,
            Err(_) => return,
        };

        struct Route {
            iface: String,
            dest: u32,
            mask: u32,
        }

        let routes: Vec<Route> = route
            .lines()
            .skip(1)
            .filter_map(|line| {
                let p: Vec<&str> = line.split_whitespace().collect();
                if p.len() < 8 {
                    return None;
                }
                let iface = p[0].to_string();
                let dest = u32::from_str_radix(p[1], 16).ok()?;
                let mask = u32::from_str_radix(p[7], 16).ok()?;
                Some(Route { iface, dest, mask })
            })
            .collect();

        for ip_str in local_ips {
            if let Ok(ip) = ip_str.parse::<std::net::Ipv4Addr>() {
                let ip_u32 = u32::from(ip);
                // Find most-specific route (highest mask) matching this IP
                let best = routes
                    .iter()
                    .filter(|r| (ip_u32 & r.mask) == r.dest)
                    .max_by_key(|r| r.mask.count_ones());
                if let Some(route) = best {
                    self.iface_ips.insert(ip_str, route.iface.clone());
                }
            }
        }

        // IPv6: /proc/net/if_inet6 already handled in refresh_iface_ips
    }

    fn refresh_process_map(&mut self) -> Result<()> {
        self.inode_to_pid.clear();
        self.pid_to_name.clear();
        self.pid_to_user.clear();

        let Ok(entries) = fs::read_dir("/proc") else {
            return Ok(());
        };

        for entry in entries.flatten() {
            let fname = entry.file_name();
            let pid_str = fname.to_string_lossy();
            let Ok(pid) = pid_str.parse::<u32>() else {
                continue;
            };

            let fd_path = format!("/proc/{}/fd", pid);
            if let Ok(fds) = fs::read_dir(&fd_path) {
                for fd in fds.flatten() {
                    let link = format!("/proc/{}/fd/{}", pid, fd.file_name().to_string_lossy());
                    if let Ok(target) = fs::read_link(&link) {
                        let t = target.to_string_lossy();
                        if let Some(rest) = t.strip_prefix("socket:[") {
                            if let Some(inode_str) = rest.strip_suffix(']') {
                                if let Ok(inode) = inode_str.parse::<u64>() {
                                    self.inode_to_pid.insert(inode, pid);
                                }
                            }
                        }
                    }
                }
            }

            let comm_path = format!("/proc/{}/comm", pid);
            if let Ok(name) = fs::read_to_string(&comm_path) {
                self.pid_to_name.insert(pid, name.trim().to_string());
            }

            let status_path = format!("/proc/{}/status", pid);
            if let Ok(status) = fs::read_to_string(&status_path) {
                for sline in status.lines() {
                    if let Some(uid_str) = sline.strip_prefix("Uid:\t") {
                        let uid: u32 = uid_str
                            .split_whitespace()
                            .next()
                            .unwrap_or("0")
                            .parse()
                            .unwrap_or(0);
                        let username = uid_to_username(uid);
                        self.pid_to_user.insert(pid, username);
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// Resolve the interface name for a given IP address.
    /// Returns "any" for wildcard addresses, looks up the iface_ips map,
    /// and falls back to "unknown" if not found.
    fn resolve_iface(&self, ip: &str) -> String {
        if ip == "0.0.0.0" || ip == "::" || ip == "::1" || ip == "127.0.0.1" {
            return "any".to_string();
        }
        self.iface_ips
            .get(ip)
            .cloned()
            .unwrap_or_else(|| "unknown".to_string())
    }

    pub fn get_open_ports(&self, iface_filter: &str) -> Vec<OpenPort> {
        let mut ports = Vec::new();
        self.collect_tcp_ports(&mut ports);
        self.collect_udp_ports(&mut ports);
        if iface_filter != "all" {
            ports.retain(|p| {
                p.interface == iface_filter || p.local_addr == "0.0.0.0" || p.local_addr == "::"
            });
        }
        ports
    }

    fn collect_tcp_ports(&self, ports: &mut Vec<OpenPort>) {
        if let Ok(tcp) = procfs::net::tcp() {
            for entry in tcp {
                if entry.state != TcpState::Listen {
                    continue;
                }
                let local_port = entry.local_address.port();
                let local_ip = entry.local_address.ip().to_string();
                let inode = entry.inode;
                let pid = self.inode_to_pid.get(&inode).copied();
                ports.push(OpenPort {
                    port: local_port,
                    protocol: "TCP",
                    interface: self.resolve_iface(&local_ip),
                    local_addr: local_ip,
                    pid,
                    process_name: pid.and_then(|p| self.pid_to_name.get(&p).cloned()),
                    user: pid.and_then(|p| self.pid_to_user.get(&p).cloned()),
                    inode,
                });
            }
        }

        if let Ok(tcp6) = procfs::net::tcp6() {
            for entry in tcp6 {
                if entry.state != TcpState::Listen {
                    continue;
                }
                let local_port = entry.local_address.port();
                let local_ip = entry.local_address.ip().to_string();
                let inode = entry.inode;
                let pid = self.inode_to_pid.get(&inode).copied();
                ports.push(OpenPort {
                    port: local_port,
                    protocol: "TCP6",
                    interface: self.resolve_iface(&local_ip),
                    local_addr: local_ip,
                    pid,
                    process_name: pid.and_then(|p| self.pid_to_name.get(&p).cloned()),
                    user: pid.and_then(|p| self.pid_to_user.get(&p).cloned()),
                    inode,
                });
            }
        }
    }

    fn collect_udp_ports(&self, ports: &mut Vec<OpenPort>) {
        if let Ok(udp) = procfs::net::udp() {
            for entry in udp {
                let local_port = entry.local_address.port();
                if local_port == 0 {
                    continue;
                }
                let local_ip = entry.local_address.ip().to_string();
                let inode = entry.inode;
                let pid = self.inode_to_pid.get(&inode).copied();
                ports.push(OpenPort {
                    port: local_port,
                    protocol: "UDP",
                    interface: self.resolve_iface(&local_ip),
                    local_addr: local_ip,
                    pid,
                    process_name: pid.and_then(|p| self.pid_to_name.get(&p).cloned()),
                    user: pid.and_then(|p| self.pid_to_user.get(&p).cloned()),
                    inode,
                });
            }
        }
    }

    pub fn get_connections(&mut self, iface_filter: &str) -> Vec<Connection> {
        let mut conns = Vec::new();
        self.collect_tcp_connections(&mut conns);
        if iface_filter != "all" {
            conns.retain(|c| c.interface == iface_filter);
        }
        conns
    }

    fn collect_tcp_connections(&mut self, conns: &mut Vec<Connection>) {
        let Ok(tcp) = procfs::net::tcp() else {
            return;
        };

        // Collect the set of locally-listening ports. A connection whose
        // local_port is in this set was accepted by a local server → incoming.
        let listening_ports: std::collections::HashSet<u16> = tcp
            .iter()
            .filter(|e| e.state == TcpState::Listen)
            .map(|e| e.local_address.port())
            .collect();

        // Count occurrences per remote endpoint
        let mut seen: HashMap<String, u32> = HashMap::new();
        for entry in &tcp {
            if entry.state != TcpState::Established {
                continue;
            }
            let remote_ip = entry.remote_address.ip().to_string();
            if remote_ip == "0.0.0.0" || remote_ip == "127.0.0.1" {
                continue;
            }
            let key = format!("{}:{}", remote_ip, entry.remote_address.port());
            *seen.entry(key).or_insert(0) += 1;
        }

        for entry in &tcp {
            if entry.state != TcpState::Established {
                continue;
            }
            let remote_ip = entry.remote_address.ip().to_string();
            if remote_ip == "0.0.0.0" || remote_ip == "127.0.0.1" {
                continue;
            }
            let remote_port = entry.remote_address.port();
            let local_port = entry.local_address.port();
            let local_ip = entry.local_address.ip().to_string();
            let inode = entry.inode;
            let key = format!("{}:{}", remote_ip, remote_port);
            let count = seen.get(&key).copied().unwrap_or(1);
            let pid = self.inode_to_pid.get(&inode).copied();
            let bps = self.estimate_bps(&key, 0);

            // A connection is incoming if our local port is one we're listening on.
            // Otherwise the kernel assigned an ephemeral port → we initiated → outgoing.
            let is_outgoing = !listening_ports.contains(&local_port);

            if !conns
                .iter()
                .any(|c: &Connection| c.remote_addr == remote_ip && c.remote_port == remote_port)
            {
                conns.push(Connection {
                    remote_addr: remote_ip.clone(),
                    remote_port,
                    local_port,
                    protocol: "TCP",
                    interface: self.resolve_iface(&local_ip),
                    bytes_per_sec: bps,
                    connections: count,
                    pid,
                    process_name: pid.and_then(|p| self.pid_to_name.get(&p).cloned()),
                    inode,
                    is_outgoing,
                });
            }
        }
    }

    fn estimate_bps(&mut self, key: &str, current_bytes: u64) -> f64 {
        let now = Instant::now();
        if let Some((prev_bytes, prev_time)) = self.connection_bytes.get(key) {
            let elapsed = now.duration_since(*prev_time).as_secs_f64();
            let rate = if elapsed > 0.0 {
                (current_bytes.saturating_sub(*prev_bytes) as f64) / elapsed
            } else {
                0.0
            };
            self.connection_bytes
                .insert(key.to_string(), (current_bytes, now));
            rate
        } else {
            self.connection_bytes
                .insert(key.to_string(), (current_bytes, now));
            0.0
        }
    }

    pub fn interface_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .interfaces
            .keys()
            .filter(|n| n.as_str() != "all")
            .cloned()
            .collect();
        names.sort();
        names.insert(0, "all".to_string());
        names
    }
}

fn uid_to_username(uid: u32) -> String {
    if let Ok(content) = fs::read_to_string("/etc/passwd") {
        for line in content.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 {
                if let Ok(u) = parts[2].parse::<u32>() {
                    if u == uid {
                        return parts[0].to_string();
                    }
                }
            }
        }
    }
    uid.to_string()
}

fn is_ipv4(s: &str) -> bool {
    s.parse::<std::net::Ipv4Addr>().is_ok()
}

fn hex_to_ipv6(hex: &str) -> Option<String> {
    if hex.len() != 32 {
        return None;
    }
    let mut groups = Vec::new();
    for i in 0..8 {
        let chunk = &hex[i * 4..(i + 1) * 4];
        groups.push(chunk);
    }
    // Parse as IpAddr for proper formatting
    let bytes: Vec<u8> = (0..16)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap_or(0))
        .collect();
    let arr: [u8; 16] = bytes.try_into().ok()?;
    let addr = std::net::Ipv6Addr::from(arr);
    Some(addr.to_string())
}

pub fn format_bytes(bytes: f64) -> String {
    if bytes >= 1_000_000_000.0 {
        format!("{:.2} Gbps", bytes / 1_000_000_000.0)
    } else if bytes >= 1_000_000.0 {
        format!("{:.2} Mbps", bytes / 1_000_000.0)
    } else if bytes >= 1_000.0 {
        format!("{:.1} Kbps", bytes / 1_000.0)
    } else {
        format!("{:.0} bps", bytes)
    }
}
