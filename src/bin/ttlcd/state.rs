//! Display modes, the settings the daemon keeps, and how they are saved and loaded.

use super::*;

// ─── Config file ──────────────────────────────────────────────────────────────

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub(crate) struct Config {
    pub(crate) device: ConfigDevice,
    pub(crate) daemon: ConfigDaemon,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub(crate) struct ConfigDevice {
    pub(crate) vid: Option<String>,
    pub(crate) pid: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub(crate) struct ConfigDaemon {
    pub(crate) port: Option<u16>,
    pub(crate) bind: Option<String>,
    pub(crate) mode: Option<String>,
    pub(crate) quality: Option<u8>,
    pub(crate) rotate: Option<u16>,
    pub(crate) headless: Option<bool>,
    /// Comma-separated or array of mode names to cycle through.
    pub(crate) rotation_modes: Option<Vec<String>>,
    /// Seconds between auto-rotation advances (0 = disabled).
    pub(crate) rotation_interval: Option<u64>,
}

pub(crate) fn load_config() -> Config {
    let Some(p) = dirs_config_path() else {
        return Config::default();
    };
    match std::fs::read_to_string(&p) {
        Err(_) => Config::default(),
        Ok(text) => match toml::from_str::<Config>(&text) {
            Ok(c) => {
                log::info!("loaded config from {}", p.display());
                c
            }
            Err(e) => {
                log::warn!("config parse error ({}): {e}", p.display());
                Config::default()
            }
        },
    }
}

pub(crate) fn dirs_config_path() -> Option<PathBuf> {
    let base = std::env::var("XDG_CONFIG_HOME").map_or_else(
        |_| {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(home).join(".config")
        },
        PathBuf::from,
    );
    if base.as_os_str().is_empty() {
        return None;
    }
    Some(base.join("ttlcd").join("config.toml"))
}

// ─── Preset persistence ───────────────────────────────────────────────────────

pub(crate) fn presets_path() -> Option<PathBuf> {
    let p = dirs_config_path()?;
    Some(p.parent()?.join("presets.json"))
}

pub(crate) fn load_presets_from_disk() -> HashMap<String, CustomTextPayload> {
    (|| -> Option<HashMap<String, CustomTextPayload>> {
        let path = presets_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    })()
    .unwrap_or_default()
}

pub(crate) fn save_presets_to_disk(presets: &HashMap<String, CustomTextPayload>) {
    let Some(path) = presets_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(presets) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&path, &text) {
                log::warn!("could not save presets to {}: {e}", path.display());
            }
        }
        Err(e) => log::warn!("could not serialise presets: {e}"),
    }
}

// ─── Persistent daemon state ────────────────────────────────────────────────

/// Path to ~/.config/ttlcd/daemon_state.json
pub(crate) fn daemon_state_path() -> Option<PathBuf> {
    let p = dirs_config_path()?;
    Some(p.parent()?.join("daemon_state.json"))
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub(crate) struct PersistedState {
    /// Last active non-transient display mode.
    pub(crate) mode: Option<String>,
    /// JPEG quality.
    pub(crate) quality: Option<u8>,
    /// Rotation degrees (0 / 90 / 180 / 270).
    pub(crate) rotate_deg: Option<u16>,
    /// Brightness factor 0.0–1.0.
    pub(crate) brightness: Option<f32>,
    /// Pet cat coat / environment / background colour.
    #[serde(default)]
    pub(crate) pet: Option<PetSettings>,
    /// Whether RGB lighting follows the panel.
    #[serde(default)]
    pub(crate) lights_enabled: Option<bool>,
}

/// Pet cat settings as exposed by `/api/pet` and stored in the state file.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct PetSettings {
    pub(crate) coat: String,
    pub(crate) environment: String,
    /// `#rrggbb`, used by the `solid` environment.
    pub(crate) background: String,
}

impl PetSettings {
    pub(crate) fn current() -> Self {
        let cfg = ttlcd::pet::config();
        Self {
            coat: cfg.coat.as_str().to_string(),
            environment: cfg.environment.as_str().to_string(),
            background: ttlcd::pet::format_hex_color(cfg.background),
        }
    }

    /// Apply to the live pet, ignoring any field that doesn't parse.
    pub(crate) fn apply(&self) {
        let mut cfg = ttlcd::pet::config();
        if let Some(c) = ttlcd::pet::Coat::from_str_name(&self.coat) {
            cfg.coat = c;
        }
        if let Some(e) = ttlcd::pet::Environment::from_str_name(&self.environment) {
            cfg.environment = e;
        }
        if let Some(b) = ttlcd::pet::parse_hex_color(&self.background) {
            cfg.background = b;
        }
        ttlcd::pet::set_config(cfg);
    }
}

pub(crate) fn load_persisted_state() -> PersistedState {
    (|| -> Option<PersistedState> {
        let path = daemon_state_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    })()
    .unwrap_or_default()
}

pub(crate) fn save_persisted_state(ps: &PersistedState) {
    let Some(path) = daemon_state_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(ps) {
        let _ = std::fs::write(&path, &text);
    }
}

/// Snapshot the current daemon settings to disk (called on any settings change).
pub(crate) fn persist_state(state: &DaemonState) {
    let ps = PersistedState {
        mode: Some(state.mode.lock().unwrap().as_str().to_string()),
        quality: Some(*state.quality.lock().unwrap()),
        rotate_deg: Some(*state.rotate_deg.lock().unwrap()),
        brightness: Some(*state.dim_factor.lock().unwrap()),
        pet: Some(PetSettings::current()),
        lights_enabled: Some(state.lights.lock().unwrap().enabled),
    };
    save_persisted_state(&ps);
}

// ─── Display modes ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)] // variants constructed via from_str_name string matching
pub(crate) enum DisplayMode {
    Clock,
    BigClock,
    ClockSysinfo,
    Sysinfo,
    CpuDetail,
    Network,
    DiskUsage,
    ProcessTop,
    Temperature,
    MemoryDetail,
    Countdown,
    CpuGraph,
    NetGraph,
    RecentAlerts,
    ProcessMemory,
    DiskIo,
    Calendar,
    Stopwatch,
    Metrics,
    Weather,
    Notes,
    QrCode,
    Pomodoro,
    Battery,
    WorldClock,
    Ticker,
    HttpStatus,
    ShellCmd,
    PingMonitor,
    Dashboard,
    LogTail,
    CpuFreq,
    HostnameIp,
    RssFeed,
    ServiceStatus,
    NetConnections,
    UptimeFancy,
    OsInfo,
    DockerStatus,
    LoggedInUsers,
    LoadGraph,
    MemGraph,
    Progress,
    MatrixRain,
    TempGraph,
    DiskIoGraph,
    GraphsOverview,
    AnalogClock,
    PingGraph,
    StatusOverview,
    GameOfLife,
    LedClock,
    BounceBall,
    NetSpeedGraph,
    CpuCoreBars,
    SpeedometerGauge,
    DiskPie,
    ThermalDashboard,
    Plasma,
    Fireworks,
    CpuHeatmap,
    Starfield,
    Pulse,
    MinimalClock,
    ParticleClock,
    Snake,
    Cube3d,
    DvdBounce,
    BinaryClock,
    Radar,
    Oscilloscope,
    Lissajous,
    Pong,
    NeonClock,
    NetHeatmap,
    WordClock,
    Breakout,
    Spectrum,
    DoomFire,
    Boids,
    Tunnel,
    Aurora,
    Metaballs,
    Sand,
    Gravity,
    Voronoi,
    ReactionDiffusion,
    Snowfall,
    HexGrid,
    BubbleChart,
    ConwayColors,
    JuliaSet,
    Confetti,
    Cpu3dBars,
    SystemHealth,
    NeonGrid,
    Ripple,
    Hypno,
    Lorenz,
    Mandelbrot,
    Kaleidoscope,
    DnaHelix,
    RainGlass,
    PerlinTerrain,
    FireText,
    DiscoFloor,
    Heartbeat,
    ClockFaces,
    WaveformBars,
    CitySkyline,
    NeonSign,
    GradientClock,
    Galaxy,
    OceanWaves,
    WarpSpeed,
    MorphingBlobs,
    ClockMatrix,
    Sierpinski,
    LavaLamp,
    PendulumWave,
    FiberOptic,
    NetworkGlobe,
    PlasmaBall,
    MossGrowth,
    Torus,
    Lemniscate,
    SystemArt,
    FourierEpicycles,
    Rule30,
    NbodyGravity,
    DoublePendulum,
    RoseCurve,
    Truchet,
    Mandala,
    FlowField,
    GlitchArt,
    WaveInterference,
    CliffordAttractor,
    Icosahedron,
    NeuralNet,
    RibbonDance,
    HexPlasma,
    SandWaterfall,
    MagneticField,
    SmokePlume,
    ColorMosaic,
    SolarSystem,
    LightningStorm,
    Spirograph,
    FractalTree,
    BubbleRise,
    AsteroidField,
    PetCat,
    EkgStrip,
    Idle,
    CustomImage,
    CustomText,
    Slideshow,
}

impl DisplayMode {
    pub(crate) const ALL_NAMES: &'static [&'static str] = &[
        "clock",
        "big_clock",
        "clock_sysinfo",
        "sysinfo",
        "cpu_detail",
        "network",
        "disk_usage",
        "process_top",
        "temperature",
        "memory_detail",
        "countdown",
        "cpu_graph",
        "net_graph",
        "recent_alerts",
        "process_memory",
        "disk_io",
        "calendar",
        "stopwatch",
        "metrics",
        "weather",
        "notes",
        "qr_code",
        "pomodoro",
        "battery",
        "world_clock",
        "ticker",
        "http_status",
        "shell_cmd",
        "ping_monitor",
        "dashboard",
        "log_tail",
        "cpu_freq",
        "hostname_ip",
        "rss_feed",
        "service_status",
        "net_connections",
        "uptime_fancy",
        "os_info",
        "docker_status",
        "logged_in_users",
        "load_graph",
        "mem_graph",
        "progress",
        "matrix_rain",
        "temp_graph",
        "disk_io_graph",
        "graphs_overview",
        "analog_clock",
        "ping_graph",
        "status_overview",
        "game_of_life",
        "led_clock",
        "bounce_ball",
        "net_speed_graph",
        "cpu_core_bars",
        "speedometer_gauge",
        "disk_pie",
        "thermal_dashboard",
        "plasma",
        "fireworks",
        "cpu_heatmap",
        "starfield",
        "pulse",
        "minimal_clock",
        "particle_clock",
        "snake",
        "cube_3d",
        "dvd_bounce",
        "binary_clock",
        "radar",
        "oscilloscope",
        "lissajous",
        "pong",
        "neon_clock",
        "net_heatmap",
        "word_clock",
        "breakout",
        "spectrum",
        "doom_fire",
        "boids",
        "tunnel",
        "aurora",
        "metaballs",
        "sand",
        "gravity",
        "voronoi",
        "reaction_diffusion",
        "snowfall",
        "hex_grid",
        "bubble_chart",
        "conway_colors",
        "julia_set",
        "confetti",
        "cpu_3d_bars",
        "system_health",
        "neon_grid",
        "ripple",
        "hypno",
        "lorenz",
        "mandelbrot",
        "kaleidoscope",
        "dna_helix",
        "rain_glass",
        "perlin_terrain",
        "fire_text",
        "disco_floor",
        "heartbeat",
        "clock_faces",
        "waveform_bars",
        "city_skyline",
        "neon_sign",
        "gradient_clock",
        "galaxy",
        "ocean_waves",
        "warp_speed",
        "morphing_blobs",
        "clock_matrix",
        "sierpinski",
        "lava_lamp",
        "pendulum_wave",
        "fiber_optic",
        "network_globe",
        "plasma_ball",
        "moss_growth",
        "torus",
        "lemniscate",
        "system_art",
        "fourier_epicycles",
        "rule_30",
        "nbody_gravity",
        "double_pendulum",
        "rose_curve",
        "truchet",
        "mandala",
        "flow_field",
        "glitch_art",
        "wave_interference",
        "clifford_attractor",
        "icosahedron",
        "neural_net",
        "ribbon_dance",
        "hex_plasma",
        "sand_waterfall",
        "magnetic_field",
        "smoke_plume",
        "color_mosaic",
        "solar_system",
        "lightning_storm",
        "spirograph",
        "fractal_tree",
        "bubble_rise",
        "asteroid_field",
        "pet_cat",
        "ekg_strip",
        "idle",
    ];

    pub(crate) fn from_str_name(s: &str) -> Option<Self> {
        match s {
            "clock" => Some(DisplayMode::Clock),
            "big_clock" => Some(DisplayMode::BigClock),
            "clock_sysinfo" => Some(DisplayMode::ClockSysinfo),
            "sysinfo" => Some(DisplayMode::Sysinfo),
            "cpu_detail" => Some(DisplayMode::CpuDetail),
            "network" => Some(DisplayMode::Network),
            "disk_usage" => Some(DisplayMode::DiskUsage),
            "process_top" => Some(DisplayMode::ProcessTop),
            "temperature" => Some(DisplayMode::Temperature),
            "memory_detail" => Some(DisplayMode::MemoryDetail),
            "countdown" => Some(DisplayMode::Countdown),
            "cpu_graph" => Some(DisplayMode::CpuGraph),
            "net_graph" => Some(DisplayMode::NetGraph),
            "recent_alerts" => Some(DisplayMode::RecentAlerts),
            "process_memory" => Some(DisplayMode::ProcessMemory),
            "disk_io" => Some(DisplayMode::DiskIo),
            "calendar" => Some(DisplayMode::Calendar),
            "stopwatch" => Some(DisplayMode::Stopwatch),
            "metrics" => Some(DisplayMode::Metrics),
            "weather" => Some(DisplayMode::Weather),
            "notes" => Some(DisplayMode::Notes),
            "qr_code" => Some(DisplayMode::QrCode),
            "pomodoro" => Some(DisplayMode::Pomodoro),
            "battery" => Some(DisplayMode::Battery),
            "world_clock" => Some(DisplayMode::WorldClock),
            "ticker" => Some(DisplayMode::Ticker),
            "http_status" => Some(DisplayMode::HttpStatus),
            "shell_cmd" => Some(DisplayMode::ShellCmd),
            "ping_monitor" => Some(DisplayMode::PingMonitor),
            "dashboard" => Some(DisplayMode::Dashboard),
            "log_tail" => Some(DisplayMode::LogTail),
            "cpu_freq" => Some(DisplayMode::CpuFreq),
            "hostname_ip" => Some(DisplayMode::HostnameIp),
            "rss_feed" => Some(DisplayMode::RssFeed),
            "service_status" => Some(DisplayMode::ServiceStatus),
            "net_connections" => Some(DisplayMode::NetConnections),
            "uptime_fancy" => Some(DisplayMode::UptimeFancy),
            "os_info" => Some(DisplayMode::OsInfo),
            "docker_status" => Some(DisplayMode::DockerStatus),
            "logged_in_users" => Some(DisplayMode::LoggedInUsers),
            "load_graph" => Some(DisplayMode::LoadGraph),
            "mem_graph" => Some(DisplayMode::MemGraph),
            "progress" => Some(DisplayMode::Progress),
            "matrix_rain" => Some(DisplayMode::MatrixRain),
            "temp_graph" => Some(DisplayMode::TempGraph),
            "disk_io_graph" => Some(DisplayMode::DiskIoGraph),
            "graphs_overview" => Some(DisplayMode::GraphsOverview),
            "analog_clock" => Some(DisplayMode::AnalogClock),
            "ping_graph" => Some(DisplayMode::PingGraph),
            "status_overview" => Some(DisplayMode::StatusOverview),
            "game_of_life" => Some(DisplayMode::GameOfLife),
            "led_clock" => Some(DisplayMode::LedClock),
            "bounce_ball" => Some(DisplayMode::BounceBall),
            "net_speed_graph" => Some(DisplayMode::NetSpeedGraph),
            "cpu_core_bars" => Some(DisplayMode::CpuCoreBars),
            "speedometer_gauge" => Some(DisplayMode::SpeedometerGauge),
            "disk_pie" => Some(DisplayMode::DiskPie),
            "thermal_dashboard" => Some(DisplayMode::ThermalDashboard),
            "plasma" => Some(DisplayMode::Plasma),
            "fireworks" => Some(DisplayMode::Fireworks),
            "cpu_heatmap" => Some(DisplayMode::CpuHeatmap),
            "starfield" => Some(DisplayMode::Starfield),
            "pulse" => Some(DisplayMode::Pulse),
            "minimal_clock" => Some(DisplayMode::MinimalClock),
            "particle_clock" => Some(DisplayMode::ParticleClock),
            "snake" => Some(DisplayMode::Snake),
            "cube_3d" => Some(DisplayMode::Cube3d),
            "dvd_bounce" => Some(DisplayMode::DvdBounce),
            "binary_clock" => Some(DisplayMode::BinaryClock),
            "radar" => Some(DisplayMode::Radar),
            "oscilloscope" => Some(DisplayMode::Oscilloscope),
            "lissajous" => Some(DisplayMode::Lissajous),
            "pong" => Some(DisplayMode::Pong),
            "neon_clock" => Some(DisplayMode::NeonClock),
            "net_heatmap" => Some(DisplayMode::NetHeatmap),
            "word_clock" => Some(DisplayMode::WordClock),
            "breakout" => Some(DisplayMode::Breakout),
            "spectrum" => Some(DisplayMode::Spectrum),
            "doom_fire" => Some(DisplayMode::DoomFire),
            "boids" => Some(DisplayMode::Boids),
            "tunnel" => Some(DisplayMode::Tunnel),
            "aurora" => Some(DisplayMode::Aurora),
            "metaballs" => Some(DisplayMode::Metaballs),
            "sand" => Some(DisplayMode::Sand),
            "gravity" => Some(DisplayMode::Gravity),
            "voronoi" => Some(DisplayMode::Voronoi),
            "reaction_diffusion" => Some(DisplayMode::ReactionDiffusion),
            "snowfall" => Some(DisplayMode::Snowfall),
            "hex_grid" => Some(DisplayMode::HexGrid),
            "bubble_chart" => Some(DisplayMode::BubbleChart),
            "conway_colors" => Some(DisplayMode::ConwayColors),
            "julia_set" => Some(DisplayMode::JuliaSet),
            "confetti" => Some(DisplayMode::Confetti),
            "cpu_3d_bars" => Some(DisplayMode::Cpu3dBars),
            "system_health" => Some(DisplayMode::SystemHealth),
            "neon_grid" => Some(DisplayMode::NeonGrid),
            "ripple" => Some(DisplayMode::Ripple),
            "hypno" => Some(DisplayMode::Hypno),
            "lorenz" => Some(DisplayMode::Lorenz),
            "mandelbrot" => Some(DisplayMode::Mandelbrot),
            "kaleidoscope" => Some(DisplayMode::Kaleidoscope),
            "dna_helix" => Some(DisplayMode::DnaHelix),
            "rain_glass" => Some(DisplayMode::RainGlass),
            "perlin_terrain" => Some(DisplayMode::PerlinTerrain),
            "fire_text" => Some(DisplayMode::FireText),
            "disco_floor" => Some(DisplayMode::DiscoFloor),
            "heartbeat" => Some(DisplayMode::Heartbeat),
            "clock_faces" => Some(DisplayMode::ClockFaces),
            "waveform_bars" => Some(DisplayMode::WaveformBars),
            "city_skyline" => Some(DisplayMode::CitySkyline),
            "neon_sign" => Some(DisplayMode::NeonSign),
            "gradient_clock" => Some(DisplayMode::GradientClock),
            "galaxy" => Some(DisplayMode::Galaxy),
            "ocean_waves" => Some(DisplayMode::OceanWaves),
            "warp_speed" => Some(DisplayMode::WarpSpeed),
            "morphing_blobs" => Some(DisplayMode::MorphingBlobs),
            "clock_matrix" => Some(DisplayMode::ClockMatrix),
            "sierpinski" => Some(DisplayMode::Sierpinski),
            "lava_lamp" => Some(DisplayMode::LavaLamp),
            "pendulum_wave" => Some(DisplayMode::PendulumWave),
            "fiber_optic" => Some(DisplayMode::FiberOptic),
            "network_globe" => Some(DisplayMode::NetworkGlobe),
            "plasma_ball" => Some(DisplayMode::PlasmaBall),
            "moss_growth" => Some(DisplayMode::MossGrowth),
            "torus" => Some(DisplayMode::Torus),
            "lemniscate" => Some(DisplayMode::Lemniscate),
            "system_art" => Some(DisplayMode::SystemArt),
            "fourier_epicycles" => Some(DisplayMode::FourierEpicycles),
            "rule_30" => Some(DisplayMode::Rule30),
            "nbody_gravity" => Some(DisplayMode::NbodyGravity),
            "double_pendulum" => Some(DisplayMode::DoublePendulum),
            "rose_curve" => Some(DisplayMode::RoseCurve),
            "truchet" => Some(DisplayMode::Truchet),
            "mandala" => Some(DisplayMode::Mandala),
            "flow_field" => Some(DisplayMode::FlowField),
            "glitch_art" => Some(DisplayMode::GlitchArt),
            "wave_interference" => Some(DisplayMode::WaveInterference),
            "clifford_attractor" => Some(DisplayMode::CliffordAttractor),
            "icosahedron" => Some(DisplayMode::Icosahedron),
            "neural_net" => Some(DisplayMode::NeuralNet),
            "ribbon_dance" => Some(DisplayMode::RibbonDance),
            "hex_plasma" => Some(DisplayMode::HexPlasma),
            "sand_waterfall" => Some(DisplayMode::SandWaterfall),
            "magnetic_field" => Some(DisplayMode::MagneticField),
            "smoke_plume" => Some(DisplayMode::SmokePlume),
            "color_mosaic" => Some(DisplayMode::ColorMosaic),
            "solar_system" => Some(DisplayMode::SolarSystem),
            "lightning_storm" => Some(DisplayMode::LightningStorm),
            "spirograph" => Some(DisplayMode::Spirograph),
            "fractal_tree" => Some(DisplayMode::FractalTree),
            "bubble_rise" => Some(DisplayMode::BubbleRise),
            "asteroid_field" => Some(DisplayMode::AsteroidField),
            "pet_cat" => Some(DisplayMode::PetCat),
            "ekg_strip" => Some(DisplayMode::EkgStrip),
            "idle" => Some(DisplayMode::Idle),
            "custom_text" => Some(DisplayMode::CustomText),
            "slideshow" => Some(DisplayMode::Slideshow),
            _ => None,
        }
    }

    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            DisplayMode::Clock => "clock",
            DisplayMode::BigClock => "big_clock",
            DisplayMode::ClockSysinfo => "clock_sysinfo",
            DisplayMode::Sysinfo => "sysinfo",
            DisplayMode::CpuDetail => "cpu_detail",
            DisplayMode::Network => "network",
            DisplayMode::DiskUsage => "disk_usage",
            DisplayMode::ProcessTop => "process_top",
            DisplayMode::Temperature => "temperature",
            DisplayMode::MemoryDetail => "memory_detail",
            DisplayMode::Countdown => "countdown",
            DisplayMode::CpuGraph => "cpu_graph",
            DisplayMode::NetGraph => "net_graph",
            DisplayMode::RecentAlerts => "recent_alerts",
            DisplayMode::ProcessMemory => "process_memory",
            DisplayMode::DiskIo => "disk_io",
            DisplayMode::Calendar => "calendar",
            DisplayMode::Stopwatch => "stopwatch",
            DisplayMode::Metrics => "metrics",
            DisplayMode::Weather => "weather",
            DisplayMode::Notes => "notes",
            DisplayMode::QrCode => "qr_code",
            DisplayMode::Pomodoro => "pomodoro",
            DisplayMode::Battery => "battery",
            DisplayMode::WorldClock => "world_clock",
            DisplayMode::Ticker => "ticker",
            DisplayMode::HttpStatus => "http_status",
            DisplayMode::ShellCmd => "shell_cmd",
            DisplayMode::PingMonitor => "ping_monitor",
            DisplayMode::Dashboard => "dashboard",
            DisplayMode::LogTail => "log_tail",
            DisplayMode::CpuFreq => "cpu_freq",
            DisplayMode::HostnameIp => "hostname_ip",
            DisplayMode::RssFeed => "rss_feed",
            DisplayMode::ServiceStatus => "service_status",
            DisplayMode::NetConnections => "net_connections",
            DisplayMode::UptimeFancy => "uptime_fancy",
            DisplayMode::OsInfo => "os_info",
            DisplayMode::DockerStatus => "docker_status",
            DisplayMode::LoggedInUsers => "logged_in_users",
            DisplayMode::LoadGraph => "load_graph",
            DisplayMode::MemGraph => "mem_graph",
            DisplayMode::Progress => "progress",
            DisplayMode::MatrixRain => "matrix_rain",
            DisplayMode::TempGraph => "temp_graph",
            DisplayMode::DiskIoGraph => "disk_io_graph",
            DisplayMode::GraphsOverview => "graphs_overview",
            DisplayMode::AnalogClock => "analog_clock",
            DisplayMode::PingGraph => "ping_graph",
            DisplayMode::StatusOverview => "status_overview",
            DisplayMode::GameOfLife => "game_of_life",
            DisplayMode::LedClock => "led_clock",
            DisplayMode::BounceBall => "bounce_ball",
            DisplayMode::NetSpeedGraph => "net_speed_graph",
            DisplayMode::CpuCoreBars => "cpu_core_bars",
            DisplayMode::SpeedometerGauge => "speedometer_gauge",
            DisplayMode::DiskPie => "disk_pie",
            DisplayMode::ThermalDashboard => "thermal_dashboard",
            DisplayMode::Plasma => "plasma",
            DisplayMode::Fireworks => "fireworks",
            DisplayMode::CpuHeatmap => "cpu_heatmap",
            DisplayMode::Starfield => "starfield",
            DisplayMode::Pulse => "pulse",
            DisplayMode::MinimalClock => "minimal_clock",
            DisplayMode::ParticleClock => "particle_clock",
            DisplayMode::Snake => "snake",
            DisplayMode::Cube3d => "cube_3d",
            DisplayMode::DvdBounce => "dvd_bounce",
            DisplayMode::BinaryClock => "binary_clock",
            DisplayMode::Radar => "radar",
            DisplayMode::Oscilloscope => "oscilloscope",
            DisplayMode::Lissajous => "lissajous",
            DisplayMode::Pong => "pong",
            DisplayMode::NeonClock => "neon_clock",
            DisplayMode::NetHeatmap => "net_heatmap",
            DisplayMode::WordClock => "word_clock",
            DisplayMode::Breakout => "breakout",
            DisplayMode::Spectrum => "spectrum",
            DisplayMode::DoomFire => "doom_fire",
            DisplayMode::Boids => "boids",
            DisplayMode::Tunnel => "tunnel",
            DisplayMode::Aurora => "aurora",
            DisplayMode::Metaballs => "metaballs",
            DisplayMode::Sand => "sand",
            DisplayMode::Gravity => "gravity",
            DisplayMode::Voronoi => "voronoi",
            DisplayMode::ReactionDiffusion => "reaction_diffusion",
            DisplayMode::Snowfall => "snowfall",
            DisplayMode::HexGrid => "hex_grid",
            DisplayMode::BubbleChart => "bubble_chart",
            DisplayMode::ConwayColors => "conway_colors",
            DisplayMode::JuliaSet => "julia_set",
            DisplayMode::Confetti => "confetti",
            DisplayMode::Cpu3dBars => "cpu_3d_bars",
            DisplayMode::SystemHealth => "system_health",
            DisplayMode::NeonGrid => "neon_grid",
            DisplayMode::Ripple => "ripple",
            DisplayMode::Hypno => "hypno",
            DisplayMode::Lorenz => "lorenz",
            DisplayMode::Mandelbrot => "mandelbrot",
            DisplayMode::Kaleidoscope => "kaleidoscope",
            DisplayMode::DnaHelix => "dna_helix",
            DisplayMode::RainGlass => "rain_glass",
            DisplayMode::PerlinTerrain => "perlin_terrain",
            DisplayMode::FireText => "fire_text",
            DisplayMode::DiscoFloor => "disco_floor",
            DisplayMode::Heartbeat => "heartbeat",
            DisplayMode::ClockFaces => "clock_faces",
            DisplayMode::WaveformBars => "waveform_bars",
            DisplayMode::CitySkyline => "city_skyline",
            DisplayMode::NeonSign => "neon_sign",
            DisplayMode::GradientClock => "gradient_clock",
            DisplayMode::Galaxy => "galaxy",
            DisplayMode::OceanWaves => "ocean_waves",
            DisplayMode::WarpSpeed => "warp_speed",
            DisplayMode::MorphingBlobs => "morphing_blobs",
            DisplayMode::ClockMatrix => "clock_matrix",
            DisplayMode::Sierpinski => "sierpinski",
            DisplayMode::LavaLamp => "lava_lamp",
            DisplayMode::PendulumWave => "pendulum_wave",
            DisplayMode::FiberOptic => "fiber_optic",
            DisplayMode::NetworkGlobe => "network_globe",
            DisplayMode::PlasmaBall => "plasma_ball",
            DisplayMode::MossGrowth => "moss_growth",
            DisplayMode::Torus => "torus",
            DisplayMode::Lemniscate => "lemniscate",
            DisplayMode::SystemArt => "system_art",
            DisplayMode::FourierEpicycles => "fourier_epicycles",
            DisplayMode::Rule30 => "rule_30",
            DisplayMode::NbodyGravity => "nbody_gravity",
            DisplayMode::DoublePendulum => "double_pendulum",
            DisplayMode::RoseCurve => "rose_curve",
            DisplayMode::Truchet => "truchet",
            DisplayMode::Mandala => "mandala",
            DisplayMode::FlowField => "flow_field",
            DisplayMode::GlitchArt => "glitch_art",
            DisplayMode::WaveInterference => "wave_interference",
            DisplayMode::CliffordAttractor => "clifford_attractor",
            DisplayMode::Icosahedron => "icosahedron",
            DisplayMode::NeuralNet => "neural_net",
            DisplayMode::RibbonDance => "ribbon_dance",
            DisplayMode::HexPlasma => "hex_plasma",
            DisplayMode::SandWaterfall => "sand_waterfall",
            DisplayMode::MagneticField => "magnetic_field",
            DisplayMode::SmokePlume => "smoke_plume",
            DisplayMode::ColorMosaic => "color_mosaic",
            DisplayMode::SolarSystem => "solar_system",
            DisplayMode::LightningStorm => "lightning_storm",
            DisplayMode::Spirograph => "spirograph",
            DisplayMode::FractalTree => "fractal_tree",
            DisplayMode::BubbleRise => "bubble_rise",
            DisplayMode::AsteroidField => "asteroid_field",
            DisplayMode::PetCat => "pet_cat",
            DisplayMode::EkgStrip => "ekg_strip",
            DisplayMode::Idle => "idle",
            DisplayMode::CustomImage => "image",
            DisplayMode::CustomText => "custom_text",
            DisplayMode::Slideshow => "slideshow",
        }
    }

    /// True for modes where content was pushed by the user and they may want
    /// to "return" to the previous live stats mode.
    pub(crate) fn is_transient(&self) -> bool {
        matches!(
            self,
            DisplayMode::CustomText
                | DisplayMode::CustomImage
                | DisplayMode::Slideshow
                | DisplayMode::Countdown
        )
    }
}

impl std::str::FromStr for DisplayMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str_name(s).ok_or_else(|| format!("unknown mode '{s}'"))
    }
}

// ─── Stopwatch state ─────────────────────────────────────────────────────────

/// Stopwatch state kept in DaemonState.
#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct StopwatchState {
    /// Whether the stopwatch is currently counting up.
    pub(crate) running: bool,
    /// Accumulated milliseconds that were counted before the last pause.
    pub(crate) elapsed_ms: u64,
    /// Unix millisecond timestamp when the stopwatch was last started
    /// (0 when stopped / never started).
    pub(crate) started_at_unix_ms: u64,
    /// Recorded lap times (accumulated ms at the moment each lap was tapped).
    pub(crate) laps: Vec<u64>,
}

impl StopwatchState {
    pub(crate) fn new() -> Self {
        Self {
            running: false,
            elapsed_ms: 0,
            started_at_unix_ms: 0,
            laps: Vec::new(),
        }
    }

    /// Current total elapsed milliseconds (may be running).
    pub(crate) fn current_elapsed_ms(&self) -> u64 {
        if self.running && self.started_at_unix_ms > 0 {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64);
            self.elapsed_ms + now_ms.saturating_sub(self.started_at_unix_ms)
        } else {
            self.elapsed_ms
        }
    }
}

// ─── Alert threshold state ────────────────────────────────────────────────────

/// Configurable thresholds for automatic panel alerts.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct AlertThresholds {
    /// Fire an alert when global CPU usage exceeds this percentage (0 = disabled).
    pub(crate) cpu_pct: f32,
    /// Fire an alert when any temperature component exceeds this Celsius (0 = disabled).
    pub(crate) temp_c: f32,
    /// Minimum seconds between consecutive threshold alerts (rate-limit).
    pub(crate) cooldown_secs: u64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            cpu_pct: 95.0,
            temp_c: 85.0,
            cooldown_secs: 120,
        }
    }
}

// ─── Night-dim schedule ───────────────────────────────────────────────────────

/// Auto-dim the panel between `start_hour` and `end_hour` (24-h, local time).
/// Outside the window, restores the user's configured brightness.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct DimSchedule {
    /// Hour of day (0–23) when dimming begins.
    pub(crate) start_hour: u8,
    /// Hour of day (0–23) when normal brightness is restored.
    pub(crate) end_hour: u8,
    /// Brightness level to apply during the dim window (0.0–1.0).
    pub(crate) dim_level: f32,
}

// ─── HTTP monitor state ───────────────────────────────────────────────────────

/// A configured HTTP health-check endpoint.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct HttpMonitorEntry {
    pub(crate) label: String,
    pub(crate) url: String,
}

/// A configured ping host.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PingHostEntry {
    pub(crate) label: String,
    pub(crate) host: String,
}

/// A single field extracted from an HTTP JSON metrics source.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct MetricsSourceField {
    /// Label shown on the panel.
    pub(crate) label: String,
    /// Dot-notation JSON path (e.g. "cpu.usage_pct", "data.0.temp").
    pub(crate) path: String,
    /// Optional display format. "{}" = raw string value.
    #[serde(default = "default_ms_format")]
    pub(crate) format: String,
    /// Optional metric color name.
    #[serde(default = "default_ms_color")]
    pub(crate) color: String,
}
pub(crate) fn default_ms_format() -> String {
    "{}".to_string()
}
pub(crate) fn default_ms_color() -> String {
    "blue".to_string()
}

/// A configured HTTP JSON metrics source for automatic metric polling.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct MetricsSource {
    /// URL to GET (must return JSON).
    pub(crate) url: String,
    /// Panel title shown above the metrics grid.
    #[serde(default)]
    pub(crate) title: String,
    /// Fields to extract and display.
    pub(crate) fields: Vec<MetricsSourceField>,
    /// Auto-refresh interval in seconds (min 5; 0 = disabled).
    #[serde(default = "default_ms_interval")]
    pub(crate) interval_secs: u64,
}
pub(crate) fn default_ms_interval() -> u64 {
    30
}

pub(crate) fn ping_hosts_path() -> Option<PathBuf> {
    let p = dirs_config_path()?;
    Some(p.parent()?.join("ping_hosts.json"))
}

pub(crate) fn load_ping_hosts() -> Vec<PingHostEntry> {
    (|| -> Option<Vec<PingHostEntry>> {
        let path = ping_hosts_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    })()
    .unwrap_or_default()
}

pub(crate) fn save_ping_hosts(hosts: &[PingHostEntry]) {
    let Some(path) = ping_hosts_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(hosts) {
        let _ = std::fs::write(&path, text);
    }
}

pub(crate) fn http_monitors_path() -> Option<PathBuf> {
    let p = dirs_config_path()?;
    Some(p.parent()?.join("http_monitors.json"))
}

pub(crate) fn load_http_monitors() -> Vec<HttpMonitorEntry> {
    (|| -> Option<Vec<HttpMonitorEntry>> {
        let path = http_monitors_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    })()
    .unwrap_or_default()
}

pub(crate) fn save_http_monitors(monitors: &[HttpMonitorEntry]) {
    let Some(path) = http_monitors_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(monitors) {
        let _ = std::fs::write(&path, text);
    }
}
// ─── New settings persistence ────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub(crate) struct PersistedNewSettings {
    pub(crate) weather_location: String,
    pub(crate) weather_auto_secs: u64,
    pub(crate) log_tail_path: String,
    pub(crate) log_tail_n: usize,
    pub(crate) log_tail_interval: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) metrics_source: Option<MetricsSource>,
    #[serde(default)]
    pub(crate) rss_feed_url: String,
    #[serde(default)]
    pub(crate) rss_feed_interval: u64,
    #[serde(default)]
    pub(crate) service_units: Vec<String>,
    #[serde(default)]
    pub(crate) service_poll_interval: u64,
    #[serde(default)]
    pub(crate) shell_cmd: String,
    #[serde(default)]
    pub(crate) shell_cmd_interval: u64,
    #[serde(default)]
    pub(crate) http_monitors: Vec<HttpMonitorEntry>,
    #[serde(default)]
    pub(crate) ping_hosts: Vec<PingHostEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) ntfy_url: Option<String>,
    /// Screensaver timeout in seconds (0 = disabled).
    #[serde(default)]
    pub(crate) screensaver_timeout_secs: u64,
    /// Screensaver mode name (e.g. "matrix_rain").
    #[serde(default)]
    pub(crate) screensaver_mode_name: String,
    /// CPU alert threshold percent (0 = disabled).
    #[serde(default)]
    pub(crate) alert_cpu_pct: f32,
    /// Temperature alert threshold Celsius (0 = disabled).
    #[serde(default)]
    pub(crate) alert_temp_c: f32,
    /// Cooldown between threshold alerts (seconds).
    #[serde(default)]
    pub(crate) alert_cooldown_secs: u64,
}

pub(crate) fn new_settings_path() -> Option<PathBuf> {
    let p = dirs_config_path()?;
    Some(p.parent()?.join("new_settings.json"))
}

pub(crate) fn load_new_settings() -> PersistedNewSettings {
    (|| -> Option<PersistedNewSettings> {
        let path = new_settings_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    })()
    .unwrap_or_default()
}

pub(crate) fn save_new_settings(state: &DaemonState) {
    let Some(path) = new_settings_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let s = PersistedNewSettings {
        weather_location: state.weather_location.lock().unwrap().clone(),
        weather_auto_secs: *state.weather_auto_secs.lock().unwrap(),
        log_tail_path: state.log_tail_path.lock().unwrap().clone(),
        log_tail_n: *state.log_tail_n.lock().unwrap(),
        log_tail_interval: *state.log_tail_interval.lock().unwrap(),
        metrics_source: state.metrics_source.lock().unwrap().clone(),
        rss_feed_url: state.rss_feed_url.lock().unwrap().clone(),
        rss_feed_interval: *state.rss_feed_interval.lock().unwrap(),
        service_units: state.service_units.lock().unwrap().clone(),
        service_poll_interval: *state.service_poll_interval.lock().unwrap(),
        shell_cmd: state.shell_cmd_text.lock().unwrap().clone(),
        shell_cmd_interval: *state.shell_cmd_interval_secs.lock().unwrap(),
        http_monitors: state.http_monitors.lock().unwrap().clone(),
        ping_hosts: state.ping_hosts.lock().unwrap().clone(),
        ntfy_url: state.ntfy_url.lock().unwrap().clone(),
        screensaver_timeout_secs: *state.screensaver_timeout_secs.lock().unwrap(),
        screensaver_mode_name: state.screensaver_mode_name.lock().unwrap().clone(),
        alert_cpu_pct: state.alert_thresholds.lock().unwrap().cpu_pct,
        alert_temp_c: state.alert_thresholds.lock().unwrap().temp_c,
        alert_cooldown_secs: state.alert_thresholds.lock().unwrap().cooldown_secs,
    };
    if let Ok(text) = serde_json::to_string_pretty(&s) {
        let _ = std::fs::write(&path, text);
    }
}

// ─── World clock persistence ──────────────────────────────────────────────────

pub(crate) fn world_clock_path() -> Option<std::path::PathBuf> {
    let p = dirs_config_path()?;
    Some(p.parent()?.join("world_clock.json"))
}

pub(crate) fn load_world_clock() -> Vec<(String, i32)> {
    (|| -> Option<Vec<(String, i32)>> {
        let path = world_clock_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    })()
    .unwrap_or_default()
}

pub(crate) fn save_world_clock(cities: &[(String, i32)]) {
    let Some(path) = world_clock_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(cities) {
        let _ = std::fs::write(&path, &text);
    }
}

// ─── Countdown timer state ───────────────────────────────────────────────────

/// Persistent state for the countdown display mode.
#[derive(Clone, Default)]
pub(crate) struct CountdownState {
    /// Unix timestamp (seconds) when the countdown expires.
    pub(crate) target_unix: u64,
    /// Original total duration in seconds (for progress bar).
    pub(crate) total_secs: u64,
    /// Optional label shown below the clock.
    pub(crate) label: String,
    /// True once the expiry notification has been pushed to history (prevents duplicate pushes).
    pub(crate) expired_notified: bool,
}

// ─── Runtime setting persistence ─────────────────────────────────────────────

/// Write a single runtime setting to the config file.
/// This is a best-effort helper: failures are logged but not propagated.
pub(crate) fn persist_setting(key: &str, value: &str) {
    let Some(path) = dirs_config_path() else {
        return;
    };
    // Read existing TOML, or start fresh
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    // Use a simple line-based replace/append strategy
    let key_eq = format!("{key} = ");
    let new_line = format!(
        "{key} = {value}
"
    );
    let updated = if current.contains(&key_eq) {
        current
            .lines()
            .map(|l| {
                if l.starts_with(&key_eq) {
                    new_line.trim_end().to_string()
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(
                "
",
            )
            + "
"
    } else {
        // Append under [daemon] section or at EOF
        if current.contains("[daemon]") {
            current + &new_line
        } else {
            current
                + "[daemon]
" + &new_line
        }
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&path, &updated) {
        log::debug!("could not persist setting {key}={value}: {e}");
    }
}
