use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::{process::Command, str};

#[derive(Deserialize)]
struct IpInterface {
    ifname: String,
    address: Option<String>,
    addr_info: Vec<IpAddr>,
}

#[derive(Deserialize)]
struct IpAddr {
    local: String,
}

#[derive(Debug, Serialize)]
struct NetworkInterface {
    name: String,
    mac_address: Option<String>,
    ip_addresses: Vec<String>,
}

pub fn get_network_interfaces() -> Result<String> {
    let output = Command::new("ip")
        .arg("-j")
        .arg("address")
        .output()
        .map_err(|e| anyhow!("Failed to run ip: {:?}", e))?;

    let ip_interfaces: Vec<IpInterface> = serde_json::from_slice(&output.stdout)?;
    let interfaces: Vec<NetworkInterface> = ip_interfaces
        .into_iter()
        .map(|iface| NetworkInterface {
            name: iface.ifname,
            mac_address: iface.address,
            ip_addresses: iface.addr_info.into_iter().map(|addr| addr.local).collect(),
        })
        .collect();

    Ok(serde_json::to_string(&interfaces)?)
}

pub fn get_primary_mac() -> Result<String> {
    let output = Command::new("ip")
        .arg("route")
        .output()
        .map_err(|e| anyhow!("Failed to run ip route: {:?}", e))?;

    let route = str::from_utf8(&output.stdout)?;
    let iface = route
        .lines()
        .find(|l| l.contains("default"))
        .and_then(|l| {
            l.split_whitespace().find(|w| *w == "dev").and_then(|_| {
                let parts: Vec<&str> = l.split_whitespace().collect();
                parts
                    .get(parts.iter().position(|&w| w == "dev")? + 1)
                    .copied()
            })
        })
        .ok_or_else(|| anyhow!("Could not determine default interface"))?;

    let output = Command::new("cat")
        .arg(format!("/sys/class/net/{iface}/address"))
        .output()
        .map_err(|e| anyhow!("Failed to read MAC: {:?}", e))?;

    Ok(str::from_utf8(&output.stdout)?.trim().to_string())
}

pub fn get_primary_ip() -> Result<String> {
    let output = Command::new("ip")
        .arg("route")
        .output()
        .map_err(|e| anyhow!("Failed to run ip route: {:?}", e))?;

    let route = str::from_utf8(&output.stdout)?;
    let iface = route
        .lines()
        .find(|l| l.contains("default"))
        .and_then(|l| {
            l.split_whitespace().find(|w| *w == "dev").and_then(|_| {
                let parts: Vec<&str> = l.split_whitespace().collect();
                parts
                    .get(parts.iter().position(|&w| w == "dev")? + 1)
                    .copied()
            })
        })
        .ok_or_else(|| anyhow!("Could not determine default interface"))?;

    let output = Command::new("ip")
        .arg("-f")
        .arg("inet")
        .arg("addr")
        .arg("show")
        .arg(iface)
        .output()
        .map_err(|e| anyhow!("Failed to get IP address: {:?}", e))?;

    let stdout = str::from_utf8(&output.stdout)?;
    let ip = stdout
        .lines()
        .find(|line| line.trim_start().starts_with("inet "))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|s| s.split('/').next())
        .ok_or_else(|| anyhow!("Could not extract IP address"))?;

    Ok(ip.to_string())
}

pub fn get_gateway_ip() -> Result<String> {
    let output = Command::new("ip")
        .arg("route")
        .output()
        .map_err(|e| anyhow!("Failed to run ip route: {:?}", e))?;

    let stdout = str::from_utf8(&output.stdout)?;
    let ip = stdout
        .lines()
        .find(|l| l.contains("default"))
        .and_then(|l| l.split_whitespace().nth(2))
        .ok_or_else(|| anyhow!("Could not find gateway IP"))?;

    Ok(ip.to_string())
}

pub fn get_selinux_status() -> Result<String> {
    let output = Command::new("getenforce")
        .output()
        .map_err(|e| anyhow!("Failed to run getenforce: {:?}", e))?;

    Ok(str::from_utf8(&output.stdout)?.trim().to_string())
}
