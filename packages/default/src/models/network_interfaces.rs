use anyhow::{Result, anyhow};
use serde::Serialize;
use std::process::Command;
use std::str;

#[derive(Debug, Serialize)]
struct NetworkInterface {
    name: String,
    mac_address: Option<String>,
    ip_addresses: Vec<String>,
}

fn get_network_interfaces() -> Result<String> {
    let output = Command::new("ip")
        .arg("-j")
        .arg("address")
        .output()
        .map_err(|e| anyhow!("Failed to run ip: {:?}", e))?;
    let interfaces: Vec<NetworkInterface> = serde_json::from_slice::<T>(&output.stdout)?
        .into_iter()
        .map(|iface: serde_json::Value| {
            let name = iface["ifname"].as_str().unwrap_or("").to_string();
            let mac_address = iface["address"].as_str().map(String::from);
            let ip_addresses = iface["addr_info"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|a| a["local"].as_str().map(String::from))
                .collect();

            NetworkInterface {
                name,
                mac_address,
                ip_addresses,
            }
        })
        .collect();

    Ok(serde_json::to_string(&interfaces)?)
}

fn get_primary_mac() -> Result<String> {
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

fn get_primary_ip() -> Result<String> {
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

fn get_gateway_ip() -> Result<String> {
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

fn get_selinux_status() -> Result<String> {
    let output = Command::new("getenforce")
        .output()
        .map_err(|e| anyhow!("Failed to run getenforce: {:?}", e))?;

    Ok(str::from_utf8(&output.stdout)?.trim().to_string())
}
