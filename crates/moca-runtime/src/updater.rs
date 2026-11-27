use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::task::JoinSet;

const GITHUB_REPO: &str = "limitcool/moca";
const GITHUB_PROXY_PREFIXES: &[(&str, &str)] = &[
    ("gh-proxy.com", "https://gh-proxy.com/"),
    ("ghfast.top", "https://ghfast.top/"),
    ("ghproxy.net", "https://ghproxy.net/"),
];
const FULL_ASSET_NAME: &str = "Moca-windows-x86_64.zip";
const PATCH_ASSET_NAME: &str = "Moca-patch-windows-x86_64.zip";
const CF_CACHE_TTL_SECS: u64 = 86_400;
const PROBE_SAMPLE_BYTES: usize = 128 * 1024;
const CF_IPV4_CIDRS: &[([u8; 4], u8)] = &[
    ([104, 16, 0, 0], 13),
    ([104, 24, 0, 0], 14),
    ([172, 64, 0, 0], 13),
    ([162, 159, 0, 0], 16),
    ([188, 114, 96, 0], 20),
    ([198, 41, 128, 0], 17),
    ([141, 101, 64, 0], 18),
    ([108, 162, 192, 0], 18),
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdaterConfig {
    #[serde(default)]
    pub custom_mirrors: Vec<String>,
    #[serde(default)]
    pub cf_preferred_ips: Vec<String>,
    #[serde(default = "default_true")]
    pub enable_github_proxies: bool,
    #[serde(default = "default_true")]
    pub auto_optimize_cf: bool,
    #[serde(default)]
    pub cached_cf_ips: Vec<String>,
    #[serde(default)]
    pub cached_cf_updated_at: u64,
}

fn default_true() -> bool {
    true
}

impl Default for UpdaterConfig {
    fn default() -> Self {
        Self {
            custom_mirrors: split_csv_env(option_env!("MOCA_UPDATE_MIRRORS")),
            cf_preferred_ips: Vec::new(),
            enable_github_proxies: true,
            auto_optimize_cf: true,
            cached_cf_ips: Vec::new(),
            cached_cf_updated_at: 0,
        }
    }
}

impl UpdaterConfig {
    pub fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        toml::from_str(&raw).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败: {e}"))?;
        }
        let body = toml::to_string_pretty(self).map_err(|e| format!("序列化更新配置失败: {e}"))?;
        std::fs::write(path, body).map_err(|e| format!("保存更新配置失败: {e}"))
    }

    pub fn is_cf_cache_stale(&self, now_secs: u64) -> bool {
        self.cached_cf_ips.is_empty()
            || now_secs.saturating_sub(self.cached_cf_updated_at) >= CF_CACHE_TTL_SECS
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn split_csv_env(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or_default()
        .split([',', ';', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateArtifact {
    pub file: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub published_at: String,
    #[serde(default)]
    pub maa_sdk_version: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub patch: Option<UpdateArtifact>,
    #[serde(default)]
    pub full: Option<UpdateArtifact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatePackageKind {
    Patch,
    Full,
}

impl UpdatePackageKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Patch => "轻量热更",
            Self::Full => "完整包",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadCandidate {
    pub label: String,
    pub url: String,
    pub resolve_host: Option<String>,
    pub resolve_ip: Option<IpAddr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeProbeResult {
    pub label: String,
    pub latency_ms: u64,
    pub speed_kbps: u64,
    pub ip: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NodeOptimizeReport {
    pub updated_config: UpdaterConfig,
    pub results: Vec<NodeProbeResult>,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct UpdateCheckInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub tag: String,
    pub notes: String,
    pub source_name: String,
    pub latency_ms: u64,
    pub package_kind: UpdatePackageKind,
    pub artifact: UpdateArtifact,
    pub candidates: Vec<DownloadCandidate>,
    pub updated_config: Option<UpdaterConfig>,
}

/// Remove stale `.old` binary or temporary `.moca-update` artifacts left by a prior self-update.
pub fn cleanup_stale_update_files() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(app_dir) = exe.parent() else {
        return;
    };
    if let Some(name) = exe.file_name().and_then(|n| n.to_str()) {
        let old_exe = app_dir.join(format!("{name}.old"));
        if old_exe.exists() {
            let _ = std::fs::remove_file(old_exe);
        }
    }
    let temp_dir = app_dir.join(".moca-update");
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}

/// Detect whether the executable runs directly from Cargo's `target/{debug,release}` build directory.
pub fn is_dev_target_exe() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let comps: Vec<String> = exe
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect();
    comps
        .windows(2)
        .any(|w| w[0] == "target" && (w[1] == "debug" || w[1] == "release" || w[1] == "deps"))
}

pub fn parse_semver(raw: &str) -> Option<semver::Version> {
    let trimmed = raw.trim().trim_start_matches(['v', 'V']);
    semver::Version::parse(trimmed).ok()
}

pub fn is_newer_version(current: &str, latest: &str) -> bool {
    match (parse_semver(current), parse_semver(latest)) {
        (Some(cur), Some(lat)) => lat > cur,
        _ => false,
    }
}

/// Sample unique host IPv4 addresses evenly across Cloudflare's public Anycast CIDR ranges.
pub fn sample_cf_candidate_ips(count: usize, seed: u64) -> Vec<IpAddr> {
    if count == 0 {
        return Vec::new();
    }
    let mut state = seed ^ 0x9e37_79b9_7f4a_7c15;
    let mut next_u64 = || {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };

    let mut out = Vec::with_capacity(count);
    let mut seen = HashSet::with_capacity(count);
    let max_attempts = count.saturating_mul(16).max(64);
    for idx in 0..max_attempts {
        if out.len() >= count {
            break;
        }
        let (base_octets, prefix) = CF_IPV4_CIDRS[idx % CF_IPV4_CIDRS.len()];
        let host_bits = 32u8.saturating_sub(prefix);
        if prefix == 0 || host_bits < 2 {
            continue;
        }
        let base = u32::from_be_bytes(base_octets);
        let mask = !((1u32 << host_bits) - 1);
        let net = base & mask;
        let host_range = 1u64 << host_bits;
        let offset = (next_u64() % (host_range - 2)) + 1;
        let ip_u32 = net | (offset as u32);
        let octets = ip_u32.to_be_bytes();
        if octets[3] == 0 || octets[3] == 255 {
            continue;
        }
        let ip = IpAddr::V4(Ipv4Addr::from(ip_u32));
        if seen.insert(ip) {
            out.push(ip);
        }
    }
    out
}

fn collect_unique_ips(lists: &[&[String]]) -> Vec<IpAddr> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for list in lists {
        for raw in *list {
            if let Ok(ip) = raw.trim().parse::<IpAddr>()
                && seen.insert(ip)
            {
                out.push(ip);
            }
        }
    }
    out
}

pub fn build_candidates_for_path(
    rel_path: &str,
    github_release_url: &str,
    config: &UpdaterConfig,
) -> Vec<DownloadCandidate> {
    let mut out = Vec::new();
    let rel = rel_path.trim_start_matches('/');
    let merged_cf_ips = collect_unique_ips(&[&config.cached_cf_ips, &config.cf_preferred_ips]);

    for mirror in &config.custom_mirrors {
        let base = mirror.trim().trim_end_matches('/');
        if base.is_empty() {
            continue;
        }
        let url = format!("{base}/{rel}");
        let parsed = reqwest::Url::parse(&url).ok();
        let host = parsed
            .as_ref()
            .and_then(|u| u.host_str().map(ToString::to_string));

        if let Some(ref host_str) = host
            && host_str.parse::<IpAddr>().is_err()
        {
            for &ip in &merged_cf_ips {
                out.push(DownloadCandidate {
                    label: format!("R2 本机优选 ({ip})"),
                    url: url.clone(),
                    resolve_host: Some(host_str.clone()),
                    resolve_ip: Some(ip),
                });
            }
        }

        let host_label = host.unwrap_or_else(|| "custom".into());
        out.push(DownloadCandidate {
            label: format!("自定义源 ({host_label})"),
            url,
            resolve_host: None,
            resolve_ip: None,
        });
    }

    if config.enable_github_proxies {
        for &(name, prefix) in GITHUB_PROXY_PREFIXES {
            out.push(DownloadCandidate {
                label: format!("加速代理 ({name})"),
                url: format!("{prefix}{github_release_url}"),
                resolve_host: None,
                resolve_ip: None,
            });
        }
    }

    out.push(DownloadCandidate {
        label: "GitHub 直连".to_string(),
        url: github_release_url.to_string(),
        resolve_host: None,
        resolve_ip: None,
    });

    out
}

fn select_artifact(
    manifest: &UpdateManifest,
    local_maa_version: Option<&str>,
    app_dir: Option<&Path>,
) -> Option<(UpdatePackageKind, UpdateArtifact)> {
    let can_patch = if let Some(patch) = &manifest.patch
        && let Some(dir) = app_dir
    {
        let has_sdk = dir.join("MaaFramework.dll").is_file();
        let has_ocr = dir
            .join("games/azur-promilia/resource/model/ocr/rec.onnx")
            .is_file();
        let sdk_compatible = match (local_maa_version, manifest.maa_sdk_version.as_str()) {
            (Some(local), remote) if !remote.trim().is_empty() => {
                major_minor(local) == major_minor(remote)
            }
            _ => false,
        };
        has_sdk && has_ocr && sdk_compatible && !patch.file.trim().is_empty()
    } else {
        false
    };

    if can_patch && let Some(patch) = &manifest.patch {
        return Some((UpdatePackageKind::Patch, patch.clone()));
    }
    if let Some(full) = &manifest.full {
        return Some((UpdatePackageKind::Full, full.clone()));
    }
    manifest
        .patch
        .clone()
        .map(|p| (UpdatePackageKind::Patch, p))
}

fn major_minor(ver: &str) -> Option<(String, String)> {
    let clean = ver.trim().trim_start_matches(['v', 'V']);
    let mut parts = clean.split('.');
    let maj = parts.next()?.to_string();
    let min = parts.next()?.to_string();
    Some((maj, min))
}

fn build_http_client(
    url_str: &str,
    resolve_host: Option<&str>,
    resolve_ip: Option<IpAddr>,
    timeout_secs: u64,
) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .user_agent(concat!("Moca-Updater/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(timeout_secs));

    if let (Some(host), Some(ip)) = (resolve_host, resolve_ip) {
        let port = reqwest::Url::parse(url_str)
            .ok()
            .and_then(|u| u.port_or_known_default())
            .unwrap_or(443);
        builder = builder.resolve(host, SocketAddr::new(ip, port));
    }

    builder
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {e}"))
}

#[derive(Deserialize)]
struct GhApiAsset {
    name: String,
    size: u64,
}

#[derive(Deserialize)]
struct GhApiRelease {
    tag_name: String,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    assets: Vec<GhApiAsset>,
}

async fn fetch_manifest_candidate(
    candidate: DownloadCandidate,
) -> Result<(UpdateManifest, String, u64), String> {
    let start = Instant::now();
    let client = build_http_client(
        &candidate.url,
        candidate.resolve_host.as_deref(),
        candidate.resolve_ip,
        6,
    )?;
    let resp = client
        .get(&candidate.url)
        .send()
        .await
        .map_err(|e| format!("{} 请求失败: {e}", candidate.label))?;
    if !resp.status().is_success() {
        return Err(format!("{} HTTP {}", candidate.label, resp.status()));
    }
    let manifest: UpdateManifest = resp
        .json()
        .await
        .map_err(|e| format!("{} 清单解析失败: {e}", candidate.label))?;
    let latency = start.elapsed().as_millis() as u64;
    Ok((manifest, candidate.label, latency))
}

async fn fetch_github_api_fallback() -> Result<(UpdateManifest, String, u64), String> {
    // Brief head start so rich `latest.json` sources (with SHA-256) win first when reachable.
    tokio::time::sleep(Duration::from_millis(350)).await;
    let start = Instant::now();
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let client = build_http_client(&url, None, None, 6)?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("GitHub API 请求失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("GitHub API HTTP {}", resp.status()));
    }
    let rel: GhApiRelease = resp
        .json()
        .await
        .map_err(|e| format!("GitHub API JSON 解析失败: {e}"))?;
    let full = rel
        .assets
        .iter()
        .find(|a| a.name == FULL_ASSET_NAME)
        .map(|a| UpdateArtifact {
            file: a.name.clone(),
            size: a.size,
            sha256: String::new(),
        });
    let patch = rel
        .assets
        .iter()
        .find(|a| a.name == PATCH_ASSET_NAME)
        .map(|a| UpdateArtifact {
            file: a.name.clone(),
            size: a.size,
            sha256: String::new(),
        });
    let version = rel.tag_name.trim_start_matches(['v', 'V']).to_string();
    let manifest = UpdateManifest {
        version,
        tag: rel.tag_name,
        published_at: rel.published_at,
        maa_sdk_version: String::new(),
        notes: rel.body,
        patch,
        full,
    };
    let latency = start.elapsed().as_millis() as u64;
    Ok((manifest, "GitHub API 兜底".to_string(), latency))
}

fn cmp_probe_order(a_lat: u64, a_kbps: u64, b_lat: u64, b_kbps: u64) -> std::cmp::Ordering {
    match (a_kbps > 0, b_kbps > 0) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (true, true) => b_kbps.cmp(&a_kbps).then_with(|| a_lat.cmp(&b_lat)),
        (false, false) => a_lat.cmp(&b_lat),
    }
}

/// Sort probed candidates, prioritizing nodes with measured throughput and breaking ties by latency.
pub fn rank_probe_candidates(
    mut items: Vec<(DownloadCandidate, u64, u64)>,
) -> Vec<(DownloadCandidate, u64, u64)> {
    items.sort_by(|a, b| cmp_probe_order(a.1, a.2, b.1, b.2));
    items
}

fn format_speed(speed_kbps: u64) -> String {
    if speed_kbps >= 1024 {
        format!("{:.1} MB/s", speed_kbps as f64 / 1024.0)
    } else {
        format!("{speed_kbps} KB/s")
    }
}

async fn measure_candidate_http(
    cand: &DownloadCandidate,
    validate_manifest: bool,
    timeout_secs: u64,
) -> Option<(u64, u64)> {
    let start = Instant::now();
    let client = build_http_client(
        &cand.url,
        cand.resolve_host.as_deref(),
        cand.resolve_ip,
        timeout_secs,
    )
    .ok()?;
    let mut req = client.get(&cand.url);
    if !validate_manifest {
        req = req.header(
            reqwest::header::RANGE,
            format!("bytes=0-{}", PROBE_SAMPLE_BYTES - 1),
        );
    }
    let resp = req.send().await.ok()?;
    let code = resp.status().as_u16();
    if code != 200 && code != 206 {
        return None;
    }

    let ttfb_ms = start.elapsed().as_millis() as u64;
    let dl_start = Instant::now();
    let mut collected = if validate_manifest {
        Vec::with_capacity(4096)
    } else {
        Vec::new()
    };
    let mut read_bytes: usize = 0;
    let mut stream = resp.bytes_stream();

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.ok()?;
        let take = chunk
            .len()
            .min(PROBE_SAMPLE_BYTES.saturating_sub(read_bytes));
        if take == 0 {
            break;
        }
        if validate_manifest {
            collected.extend_from_slice(&chunk[..take]);
        }
        read_bytes += take;
        if read_bytes >= PROBE_SAMPLE_BYTES {
            break;
        }
    }
    drop(stream);

    if validate_manifest && serde_json::from_slice::<UpdateManifest>(&collected).is_err() {
        return None;
    }

    let transfer_ms = dl_start.elapsed().as_millis() as u64;
    let speed_kbps = if read_bytes >= 16 * 1024 && transfer_ms > 0 {
        ((read_bytes as u64) * 1000 / 1024) / transfer_ms.max(1)
    } else {
        0
    };
    Some((ttfb_ms, speed_kbps))
}

/// Optimize update nodes from this client's local network:
/// 1. Two-stage Cloudflare Anycast IP selection (TCP RTT pre-screen -> HTTPS + SNI + manifest verify)
///    for configured custom/R2 mirror domains when `auto_optimize_cf` is enabled.
/// 2. Live latency/speed benchmark across default DNS mirror, GitHub proxies, and GitHub direct.
pub async fn optimize_client_nodes(config: &UpdaterConfig) -> NodeOptimizeReport {
    let mut updated_config = config.clone();
    let sem = Arc::new(tokio::sync::Semaphore::new(24));
    let mut results: Vec<NodeProbeResult> = Vec::new();
    let mut cf_cached_count: usize = 0;

    let first_mirror_probe = config.custom_mirrors.iter().find_map(|raw| {
        let base = raw.trim().trim_end_matches('/');
        if base.is_empty() {
            return None;
        }
        let probe_url = format!("{base}/latest.json");
        let parsed = reqwest::Url::parse(&probe_url).ok()?;
        let host = parsed.host_str()?.to_string();
        let port = parsed.port_or_known_default().unwrap_or(443);
        (host.parse::<IpAddr>().is_err()).then_some((probe_url, host, port))
    });

    if config.auto_optimize_cf
        && let Some((probe_url, host, port)) = first_mirror_probe
    {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x5eed_1234)
            ^ (std::process::id() as u64);
        let sampled: Vec<String> = sample_cf_candidate_ips(28, seed)
            .into_iter()
            .map(|ip| ip.to_string())
            .collect();
        let pool = collect_unique_ips(&[&config.cf_preferred_ips, &config.cached_cf_ips, &sampled]);

        let mut tcp_set = JoinSet::new();
        for ip in pool {
            let sem_c = Arc::clone(&sem);
            tcp_set.spawn(async move {
                let _permit = sem_c.acquire_owned().await.ok()?;
                let addr = SocketAddr::new(ip, port);
                let start = Instant::now();
                tokio::time::timeout(
                    Duration::from_millis(1200),
                    tokio::net::TcpStream::connect(addr),
                )
                .await
                .ok()?
                .ok()?;
                Some((ip, start.elapsed().as_millis() as u64))
            });
        }
        let mut tcp_hits = Vec::new();
        while let Some(res) = tcp_set.join_next().await {
            if let Ok(Some(hit)) = res {
                tcp_hits.push(hit);
            }
        }
        tcp_hits.sort_by_key(|(_, ms)| *ms);
        tcp_hits.truncate(6);

        let mut https_set = JoinSet::new();
        for (ip, _) in tcp_hits {
            let sem_c = Arc::clone(&sem);
            let url_c = probe_url.clone();
            let host_c = host.clone();
            https_set.spawn(async move {
                let _permit = sem_c.acquire_owned().await.ok()?;
                let cand = DownloadCandidate {
                    label: format!("R2 本机优选 ({ip})"),
                    url: url_c,
                    resolve_host: Some(host_c),
                    resolve_ip: Some(ip),
                };
                let (latency_ms, speed_kbps) = measure_candidate_http(&cand, true, 4).await?;
                Some(NodeProbeResult {
                    label: cand.label,
                    latency_ms,
                    speed_kbps,
                    ip: Some(ip.to_string()),
                })
            });
        }
        let mut cf_hits = Vec::new();
        while let Some(res) = https_set.join_next().await {
            if let Ok(Some(hit)) = res {
                cf_hits.push(hit);
            }
        }
        cf_hits.sort_by(|a, b| {
            cmp_probe_order(a.latency_ms, a.speed_kbps, b.latency_ms, b.speed_kbps)
        });

        let top_ips: Vec<String> = cf_hits
            .iter()
            .filter_map(|r| r.ip.clone())
            .take(3)
            .collect();
        cf_cached_count = top_ips.len();
        updated_config.cached_cf_ips = top_ips;
        updated_config.cached_cf_updated_at = if cf_cached_count > 0 {
            now_unix_secs()
        } else {
            0
        };
        results.extend(cf_hits);
    }

    let gh_latest_manifest =
        format!("https://github.com/{GITHUB_REPO}/releases/latest/download/latest.json");
    let baseline_cfg = UpdaterConfig {
        custom_mirrors: config.custom_mirrors.clone(),
        cf_preferred_ips: Vec::new(),
        enable_github_proxies: config.enable_github_proxies,
        auto_optimize_cf: false,
        cached_cf_ips: Vec::new(),
        cached_cf_updated_at: 0,
    };
    let baseline_cands =
        build_candidates_for_path("latest.json", &gh_latest_manifest, &baseline_cfg);
    let mut base_set = JoinSet::new();
    for cand in baseline_cands {
        let sem_c = Arc::clone(&sem);
        base_set.spawn(async move {
            let _permit = sem_c.acquire_owned().await.ok()?;
            let (latency_ms, speed_kbps) = measure_candidate_http(&cand, true, 5).await?;
            Some(NodeProbeResult {
                label: cand.label,
                latency_ms,
                speed_kbps,
                ip: None,
            })
        });
    }
    while let Some(res) = base_set.join_next().await {
        if let Ok(Some(hit)) = res {
            results.push(hit);
        }
    }

    results.sort_by(|a, b| cmp_probe_order(a.latency_ms, a.speed_kbps, b.latency_ms, b.speed_kbps));

    let summary = if let Some(best) = results.first() {
        let speed_note = if best.speed_kbps > 0 {
            format!(" · {}", format_speed(best.speed_kbps))
        } else {
            String::new()
        };
        let cf_note = if cf_cached_count > 0 {
            format!("；已本机缓存 {cf_cached_count} 个 Cloudflare 优选 IP")
        } else if !config.custom_mirrors.is_empty() && config.auto_optimize_cf {
            "；未检测到匹配 Cloudflare 优选节点，将使用默认镜像直连与代理竞速".to_string()
        } else {
            String::new()
        };
        format!(
            "最快节点：{}（{} ms{speed_note}）{cf_note}",
            best.label, best.latency_ms
        )
    } else {
        "本机节点测速未发现可用更新源，请检查网络连接或代理配置".to_string()
    };

    NodeOptimizeReport {
        updated_config,
        results,
        summary,
    }
}

/// Concurrently race all configured mirrors, client-optimized IPs, and GitHub proxies for `latest.json`.
pub async fn check_for_updates(
    current_version: &str,
    local_maa_version: Option<&str>,
    config: &UpdaterConfig,
) -> Result<UpdateCheckInfo, String> {
    let mut effective_config = config.clone();
    let mut updated_config = None;
    if config.auto_optimize_cf
        && !config.custom_mirrors.is_empty()
        && config.is_cf_cache_stale(now_unix_secs())
    {
        let report = optimize_client_nodes(config).await;
        effective_config = report.updated_config;
        updated_config = Some(effective_config.clone());
    }

    let gh_latest_manifest =
        format!("https://github.com/{GITHUB_REPO}/releases/latest/download/latest.json");
    let manifest_candidates =
        build_candidates_for_path("latest.json", &gh_latest_manifest, &effective_config);

    let mut set = JoinSet::new();
    for cand in manifest_candidates {
        set.spawn(fetch_manifest_candidate(cand));
    }
    set.spawn(fetch_github_api_fallback());

    let mut winner: Option<(UpdateManifest, String, u64)> = None;
    let mut errors = Vec::new();

    while let Some(joined) = set.join_next().await {
        match joined {
            Ok(Ok(res)) => {
                winner = Some(res);
                set.abort_all();
                break;
            }
            Ok(Err(err)) => errors.push(err),
            Err(err) => errors.push(err.to_string()),
        }
    }

    let Some((manifest, source_name, latency_ms)) = winner else {
        return Err(format!("所有更新源均不可达: {}", errors.join(" | ")));
    };

    let app_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let Some((package_kind, artifact)) =
        select_artifact(&manifest, local_maa_version, app_dir.as_deref())
    else {
        return Err("更新清单中未包含可用的 Windows 安装包".to_string());
    };

    let tag = if manifest.tag.trim().is_empty() {
        format!("v{}", manifest.version.trim_start_matches(['v', 'V']))
    } else {
        manifest.tag.clone()
    };
    let gh_asset_url = format!(
        "https://github.com/{GITHUB_REPO}/releases/download/{tag}/{}",
        artifact.file
    );
    let candidates = build_candidates_for_path(&artifact.file, &gh_asset_url, &effective_config);
    let has_update = is_newer_version(current_version, &manifest.version);

    Ok(UpdateCheckInfo {
        current_version: current_version.trim_start_matches(['v', 'V']).to_string(),
        latest_version: manifest.version.trim_start_matches(['v', 'V']).to_string(),
        has_update,
        tag,
        notes: manifest.notes,
        source_name,
        latency_ms,
        package_kind,
        artifact,
        candidates,
        updated_config,
    })
}

/// Probe all download candidates concurrently using a bounded 128KB sample to measure real speed and latency.
pub async fn probe_fastest_candidates(
    candidates: &[DownloadCandidate],
) -> Vec<(DownloadCandidate, u64, u64)> {
    let mut set = JoinSet::new();
    for cand in candidates.iter().cloned() {
        set.spawn(async move {
            let (latency_ms, speed_kbps) = measure_candidate_http(&cand, false, 4).await?;
            Some((cand, latency_ms, speed_kbps))
        });
    }

    let mut ranked = Vec::new();
    while let Some(res) = set.join_next().await {
        if let Ok(Some(hit)) = res {
            ranked.push(hit);
        }
    }
    if ranked.is_empty() {
        candidates.iter().cloned().map(|c| (c, 9999, 0)).collect()
    } else {
        rank_probe_candidates(ranked)
    }
}

/// Speed-test candidates, stream download the update archive, verify SHA-256, unpack cleanly,
/// and launch replacement/restart for the new binary.
pub async fn download_and_install_update<F>(
    info: &UpdateCheckInfo,
    on_progress: F,
) -> Result<PathBuf, String>
where
    F: Fn(u64, u64, String) + Send + Sync + 'static,
{
    if is_dev_target_exe() {
        return Err(
            "当前运行在源码开发构建目录，不支持原地自更新；请使用 Release 便携包或手动编译更新"
                .to_string(),
        );
    }

    let current_exe = std::env::current_exe().map_err(|e| format!("无法获取当前程序路径: {e}"))?;
    let app_dir = current_exe
        .parent()
        .ok_or_else(|| "无法解析应用所在目录".to_string())?
        .to_path_buf();

    let update_dir = app_dir.join(".moca-update");
    if update_dir.exists() {
        let _ = std::fs::remove_dir_all(&update_dir);
    }
    std::fs::create_dir_all(&update_dir).map_err(|e| format!("创建更新临时目录失败: {e}"))?;

    let ranked = probe_fastest_candidates(&info.candidates).await;
    let zip_path = update_dir.join("update.zip");
    let mut download_errors = Vec::new();
    let mut downloaded_ok = false;

    for (cand, latency_ms, speed_kbps) in ranked {
        let source_desc = if latency_ms < 9999 && speed_kbps > 0 {
            format!(
                "{} ({latency_ms}ms · {})",
                cand.label,
                format_speed(speed_kbps)
            )
        } else if latency_ms < 9999 {
            format!("{} ({latency_ms}ms)", cand.label)
        } else {
            cand.label.clone()
        };
        match stream_download_and_verify(
            &cand,
            &zip_path,
            info.artifact.size,
            &info.artifact.sha256,
            &source_desc,
            &on_progress,
        )
        .await
        {
            Ok(()) => {
                downloaded_ok = true;
                break;
            }
            Err(err) => {
                let _ = std::fs::remove_file(&zip_path);
                download_errors.push(format!("{}: {err}", cand.label));
            }
        }
    }

    if !downloaded_ok {
        return Err(format!(
            "更新包下载或校验失败: {}",
            download_errors.join(" | ")
        ));
    }

    let staging_dir = update_dir.join("staging");
    let zip_clone = zip_path.clone();
    let staging_clone = staging_dir.clone();
    tokio::task::spawn_blocking(move || extract_zip_safely(&zip_clone, &staging_clone))
        .await
        .map_err(|e| format!("解压线程异常: {e}"))??;

    apply_staged_update(
        info.package_kind,
        &current_exe,
        &app_dir,
        &update_dir,
        &staging_dir,
    )
}

async fn stream_download_and_verify<F>(
    cand: &DownloadCandidate,
    dest_path: &Path,
    expected_size: u64,
    expected_sha256: &str,
    source_desc: &str,
    on_progress: &F,
) -> Result<(), String>
where
    F: Fn(u64, u64, String),
{
    let client = build_http_client(
        &cand.url,
        cand.resolve_host.as_deref(),
        cand.resolve_ip,
        120,
    )?;
    let resp = client
        .get(&cand.url)
        .send()
        .await
        .map_err(|e| format!("连接失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let total = resp
        .content_length()
        .unwrap_or(expected_size)
        .max(expected_size);
    let mut file = File::create(dest_path).map_err(|e| format!("创建临时文件失败: {e}"))?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut last_notify_bytes: u64 = 0;
    let mut stream = resp.bytes_stream();

    on_progress(0, total, source_desc.to_string());

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.map_err(|e| format!("读取网络流中断: {e}"))?;
        file.write_all(&chunk)
            .map_err(|e| format!("写入临时包失败: {e}"))?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;
        if downloaded - last_notify_bytes >= 128 * 1024 {
            last_notify_bytes = downloaded;
            on_progress(downloaded, total, source_desc.to_string());
        }
    }
    file.flush().map_err(|e| format!("刷新磁盘缓存失败: {e}"))?;
    drop(file);
    on_progress(downloaded, total.max(downloaded), source_desc.to_string());

    let expected_hex = expected_sha256.trim().to_ascii_lowercase();
    if !expected_hex.is_empty() {
        let actual_hex = hex::encode(hasher.finalize());
        if actual_hex != expected_hex {
            return Err(format!(
                "SHA-256 校验失败 (期望 {expected_hex}, 实际 {actual_hex})"
            ));
        }
    }

    Ok(())
}

pub fn extract_zip_safely(zip_path: &Path, staging_dir: &Path) -> Result<(), String> {
    if staging_dir.exists() {
        std::fs::remove_dir_all(staging_dir).map_err(|e| format!("清理暂存目录失败: {e}"))?;
    }
    std::fs::create_dir_all(staging_dir).map_err(|e| format!("创建暂存目录失败: {e}"))?;

    let file = File::open(zip_path).map_err(|e| format!("打开更新压缩包失败: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("解析 ZIP 压缩包失败: {e}"))?;

    for idx in 0..archive.len() {
        let mut entry = archive
            .by_index(idx)
            .map_err(|e| format!("读取压缩条目失败: {e}"))?;
        let Some(rel_path) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };

        if let Some(Component::Normal(first)) = rel_path.components().next() {
            let root = first.to_string_lossy().to_ascii_lowercase();
            if root == "profiles" || root == "logs" {
                continue;
            }
        }

        let target = staging_dir.join(&rel_path);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| format!("创建目录失败: {e}"))?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("创建父目录失败: {e}"))?;
            }
            let mut out = File::create(&target).map_err(|e| format!("解压写出文件失败: {e}"))?;
            let mut buf = [0u8; 64 * 1024];
            loop {
                let n = entry
                    .read(&mut buf)
                    .map_err(|e| format!("解压读取失败: {e}"))?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n])
                    .map_err(|e| format!("解压写入失败: {e}"))?;
            }
        }
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

fn apply_staged_update(
    kind: UpdatePackageKind,
    current_exe: &Path,
    app_dir: &Path,
    update_dir: &Path,
    staging_dir: &Path,
) -> Result<PathBuf, String> {
    if kind == UpdatePackageKind::Patch
        && let Some(exe_name) = current_exe.file_name().and_then(|s| s.to_str())
    {
        let old_exe = app_dir.join(format!("{exe_name}.old"));
        let _ = std::fs::remove_file(&old_exe);
        if std::fs::rename(current_exe, &old_exe).is_ok() {
            if copy_dir_all(staging_dir, app_dir).is_ok() {
                let target_exe = if current_exe.exists() {
                    current_exe.to_path_buf()
                } else {
                    app_dir.join("Moca.exe")
                };
                if Command::new(&target_exe).spawn().is_ok() {
                    return Ok(target_exe);
                }
            }
            let _ = std::fs::remove_file(current_exe);
            let _ = std::fs::rename(&old_exe, current_exe);
        }
    }

    spawn_deferred_updater_script(current_exe, app_dir, update_dir, staging_dir)?;
    Ok(current_exe.to_path_buf())
}

#[cfg(windows)]
fn spawn_deferred_updater_script(
    current_exe: &Path,
    app_dir: &Path,
    update_dir: &Path,
    staging_dir: &Path,
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let pid = std::process::id();
    let script_path = update_dir.join("apply.cmd");
    let exe_str = current_exe.to_string_lossy();
    let app_str = app_dir.to_string_lossy();
    let stage_str = staging_dir.to_string_lossy();
    let upd_str = update_dir.to_string_lossy();

    let script = format!(
        "@echo off\r\n\
         setlocal\r\n\
         set \"PID={pid}\"\r\n\
         for /L %%i in (1,1,16) do (\r\n\
           tasklist /FI \"PID eq %PID%\" 2>nul | find \"%PID%\" >nul || goto apply\r\n\
           ping -n 2 127.0.0.1 >nul\r\n\
         )\r\n\
         :apply\r\n\
         xcopy /E /Y /I \"{stage_str}\\*\" \"{app_str}\\\" >nul 2>&1\r\n\
         start \"\" \"{exe_str}\"\r\n\
         rd /S /Q \"{upd_str}\" >nul 2>&1\r\n"
    );
    std::fs::write(&script_path, script).map_err(|e| format!("写出安装脚本失败: {e}"))?;

    Command::new("cmd.exe")
        .args(["/C", &script_path.to_string_lossy()])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("启动更新助手失败: {e}"))?;
    Ok(())
}

#[cfg(not(windows))]
fn spawn_deferred_updater_script(
    current_exe: &Path,
    _app_dir: &Path,
    _update_dir: &Path,
    _staging_dir: &Path,
) -> Result<(), String> {
    Command::new(current_exe)
        .spawn()
        .map_err(|e| format!("重启失败: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    #[test]
    fn compares_semver_with_and_without_v_prefix() {
        assert!(is_newer_version("0.1.5", "v0.1.6"));
        assert!(is_newer_version("v0.1.5", "0.2.0"));
        assert!(!is_newer_version("0.1.5", "v0.1.5"));
        assert!(!is_newer_version("0.1.5", "0.1.4"));
    }

    #[test]
    fn builds_candidates_with_r2_preferred_ips_and_github_fallback() {
        let cfg = UpdaterConfig {
            custom_mirrors: vec!["https://dl.example.com/".into()],
            cf_preferred_ips: vec!["104.16.10.10".into(), "invalid-ip".into()],
            enable_github_proxies: true,
            auto_optimize_cf: true,
            cached_cf_ips: vec!["172.64.20.20".into(), "104.16.10.10".into()],
            cached_cf_updated_at: 123,
        };
        let list = build_candidates_for_path(
            "Moca-patch-windows-x86_64.zip",
            "https://github.com/limitcool/moca/releases/download/v0.1.6/Moca-patch-windows-x86_64.zip",
            &cfg,
        );
        assert_eq!(list[0].label, "R2 本机优选 (172.64.20.20)");
        assert_eq!(list[0].resolve_host.as_deref(), Some("dl.example.com"));
        assert_eq!(list[0].resolve_ip, "172.64.20.20".parse().ok());
        assert_eq!(list[1].label, "R2 本机优选 (104.16.10.10)");
        assert_eq!(list[2].label, "自定义源 (dl.example.com)");
        assert!(list.iter().any(|c| c.label.contains("gh-proxy.com")));
        assert_eq!(list.last().expect("direct").label, "GitHub 直连");
    }

    #[test]
    fn samples_valid_unique_cf_ipv4_addresses() {
        let ips = sample_cf_candidate_ips(24, 42);
        assert_eq!(ips.len(), 24);
        let unique: HashSet<_> = ips.iter().copied().collect();
        assert_eq!(unique.len(), 24);

        for ip in ips {
            let IpAddr::V4(v4) = ip else {
                panic!("expected IPv4 address");
            };
            let octets = v4.octets();
            assert_ne!(octets[3], 0);
            assert_ne!(octets[3], 255);
            let addr = u32::from_be_bytes(octets);
            let in_cidr = CF_IPV4_CIDRS.iter().any(|&(base_octets, prefix)| {
                let mask = !((1u32 << (32 - prefix)) - 1);
                (addr & mask) == (u32::from_be_bytes(base_octets) & mask)
            });
            assert!(in_cidr, "ip {v4} not in CF_IPV4_CIDRS");
        }
    }

    #[test]
    fn ranks_probe_candidates_preferring_throughput_over_bare_rtt() {
        let make_cand = |name: &str| DownloadCandidate {
            label: name.to_string(),
            url: "https://example.com/a.zip".to_string(),
            resolve_host: None,
            resolve_ip: None,
        };
        let ranked = rank_probe_candidates(vec![
            (make_cand("slow-ping"), 30, 200),
            (make_cand("bare-rtt"), 15, 0),
            (make_cand("fast-dl"), 90, 4_500),
            (make_cand("mid-dl"), 60, 1_200),
        ]);
        let names: Vec<_> = ranked.iter().map(|(c, _, _)| c.label.as_str()).collect();
        assert_eq!(names, vec!["fast-dl", "mid-dl", "slow-ping", "bare-rtt"]);
    }

    #[test]
    fn deserializes_legacy_updater_config_with_auto_optimize_default() {
        let legacy = r#"
custom_mirrors = ["https://dl.example.com"]
cf_preferred_ips = ["104.16.0.1"]
enable_github_proxies = false
"#;
        let parsed: UpdaterConfig = toml::from_str(legacy).unwrap();
        assert!(parsed.auto_optimize_cf);
        assert!(parsed.cached_cf_ips.is_empty());
        assert_eq!(parsed.cached_cf_updated_at, 0);
        assert!(!parsed.enable_github_proxies);
    }

    #[test]
    fn selects_patch_only_when_sdk_and_ocr_compatible() {
        let tmp = std::env::temp_dir().join(format!("moca_upd_sel_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("games/azur-promilia/resource/model/ocr")).unwrap();
        std::fs::write(tmp.join("MaaFramework.dll"), b"dll").unwrap();
        std::fs::write(
            tmp.join("games/azur-promilia/resource/model/ocr/rec.onnx"),
            b"onnx",
        )
        .unwrap();

        let manifest = UpdateManifest {
            version: "0.1.6".into(),
            tag: "v0.1.6".into(),
            published_at: String::new(),
            maa_sdk_version: "5.16.1".into(),
            notes: String::new(),
            patch: Some(UpdateArtifact {
                file: PATCH_ASSET_NAME.into(),
                size: 5_000_000,
                sha256: "abc".into(),
            }),
            full: Some(UpdateArtifact {
                file: FULL_ASSET_NAME.into(),
                size: 55_000_000,
                sha256: "def".into(),
            }),
        };

        let (kind_ok, _) = select_artifact(&manifest, Some("5.16.1"), Some(&tmp)).unwrap();
        assert_eq!(kind_ok, UpdatePackageKind::Patch);

        let (kind_diff_sdk, _) = select_artifact(&manifest, Some("5.17.0"), Some(&tmp)).unwrap();
        assert_eq!(kind_diff_sdk, UpdatePackageKind::Full);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn extracts_zip_safely_skipping_profiles_and_traversal() {
        let tmp = std::env::temp_dir().join(format!("moca_upd_zip_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let zip_path = tmp.join("test.zip");
        let staging = tmp.join("staging");

        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = SimpleFileOptions::default();

            zip.start_file("Moca.exe", opts).unwrap();
            zip.write_all(b"new-exe").unwrap();

            zip.start_file("profiles/azur-promilia.toml", opts).unwrap();
            zip.write_all(b"overwrite-forbidden").unwrap();

            zip.start_file("logs/old.log", opts).unwrap();
            zip.write_all(b"skip-logs").unwrap();

            zip.start_file("../evil.txt", opts).unwrap();
            zip.write_all(b"traversal").unwrap();

            zip.finish().unwrap();
        }

        extract_zip_safely(&zip_path, &staging).unwrap();
        assert_eq!(std::fs::read(staging.join("Moca.exe")).unwrap(), b"new-exe");
        assert!(!staging.join("profiles").exists());
        assert!(!staging.join("logs").exists());
        assert!(!tmp.join("evil.txt").exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn roundtrips_updater_config_toml() {
        let tmp = std::env::temp_dir().join(format!("moca_upd_cfg_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let cfg_path = tmp.join("updater.toml");

        let cfg = UpdaterConfig {
            custom_mirrors: vec!["https://r2.example.com".into()],
            cf_preferred_ips: vec!["104.16.1.2".into()],
            enable_github_proxies: false,
            auto_optimize_cf: true,
            cached_cf_ips: vec!["172.64.1.2".into()],
            cached_cf_updated_at: 1_700_000_000,
        };
        cfg.save(&cfg_path).unwrap();
        let loaded = UpdaterConfig::load(&cfg_path);
        assert_eq!(loaded, cfg);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
