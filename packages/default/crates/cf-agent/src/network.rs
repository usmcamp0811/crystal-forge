//! Linux network inspection used by agent heartbeat collection.

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

pub(crate) fn get_network_interfaces() -> Result<String> {
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

fn default_interface() -> Result<String> {
    let output = Command::new("ip")
        .arg("route")
        .output()
        .map_err(|e| anyhow!("Failed to run ip route: {:?}", e))?;
    let route = str::from_utf8(&output.stdout)?;

    route
        .lines()
        .find(|line| line.contains("default"))
        .and_then(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            parts
                .iter()
                .position(|part| *part == "dev")
                .and_then(|index| parts.get(index + 1))
                .map(|interface| (*interface).to_string())
        })
        .ok_or_else(|| anyhow!("Could not determine default interface"))
}

pub(crate) fn get_primary_mac() -> Result<String> {
    let interface = default_interface()?;
    let output = Command::new("cat")
        .arg(format!("/sys/class/net/{interface}/address"))
        .output()
        .map_err(|e| anyhow!("Failed to read MAC: {:?}", e))?;

    Ok(str::from_utf8(&output.stdout)?.trim().to_string())
}

pub(crate) fn get_primary_ip() -> Result<String> {
    let interface = default_interface()?;
    let output = Command::new("ip")
        .args(["-f", "inet", "addr", "show", &interface])
        .output()
        .map_err(|e| anyhow!("Failed to get IP address: {:?}", e))?;

    let stdout = str::from_utf8(&output.stdout)?;
    stdout
        .lines()
        .find(|line| line.trim_start().starts_with("inet "))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|address| address.split('/').next())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("Could not extract IP address"))
}

pub(crate) fn get_gateway_ip() -> Result<String> {
    let output = Command::new("ip")
        .arg("route")
        .output()
        .map_err(|e| anyhow!("Failed to run ip route: {:?}", e))?;
    let stdout = str::from_utf8(&output.stdout)?;

    stdout
        .lines()
        .find(|line| line.contains("default"))
        .and_then(|line| line.split_whitespace().nth(2))
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("Could not find gateway IP"))
}

pub(crate) fn get_selinux_status() -> Result<String> {
    let output = Command::new("getenforce")
        .output()
        .map_err(|e| anyhow!("Failed to run getenforce: {:?}", e))?;

    Ok(str::from_utf8(&output.stdout)?.trim().to_string())
}
