//! Pixel-art pet cat.
//!
//! The scene is painted on a 60×60 grid and scaled ×8 to the 480×480 panel,
//! so every "pixel" on screen is an 8×8 block.  The cat is a 16×16 sprite that
//! cycles through idle, sleeping, paw-licking, stretching and ball-playing
//! animations; every animation starts and ends in the sitting pose, and pose
//! changes are cross-faded over a few frames.
//!
//! Coat and environment are runtime settings ([`config`] / [`set_config`]).

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use image::{DynamicImage, Rgb, RgbImage};

const GRID: i32 = 60;
const SCALE: u32 = 8;
const SPRITE: i32 = 16;
/// First grid row below the cat's feet.
const FLOOR_Y: i32 = 50;
/// Left edge of the cat sprite.
const CAT_X: i32 = 22;
/// Simulation step (30 Hz), independent of how often frames are rendered.
const STEP: Duration = Duration::from_millis(33);
/// Steps a pose change is cross-faded over.
const FADE_STEPS: u32 = 4;

// ─── Settings ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Coat {
    /// Black and white.
    Tuxedo,
    Black,
    /// Orange tabby.
    Orange,
}

impl Coat {
    pub const ALL: [Coat; 3] = [Coat::Tuxedo, Coat::Black, Coat::Orange];

    pub fn as_str(self) -> &'static str {
        match self {
            Coat::Tuxedo => "tuxedo",
            Coat::Black => "black",
            Coat::Orange => "orange",
        }
    }

    pub fn from_str_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Environment {
    /// Flat [`PetConfig::background`] colour.
    Solid,
    Forest,
    LivingRoom,
    /// Moonlit forest: fireflies, glowing mushrooms, bats, and now and then
    /// a witch crossing the moon.
    NightForest,
    /// Alchemy lab: bubbling colour-changing cauldron and spiders.
    Dungeon,
    /// Dark library: flickering candles and a book that flies past.
    Library,
}

impl Environment {
    pub const ALL: [Environment; 6] = [
        Environment::Solid,
        Environment::Forest,
        Environment::LivingRoom,
        Environment::NightForest,
        Environment::Dungeon,
        Environment::Library,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Environment::Solid => "solid",
            Environment::Forest => "forest",
            Environment::LivingRoom => "living_room",
            Environment::NightForest => "night_forest",
            Environment::Dungeon => "dungeon",
            Environment::Library => "library",
        }
    }

    pub fn from_str_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.as_str() == s)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PetConfig {
    pub coat: Coat,
    pub environment: Environment,
    /// Background colour for [`Environment::Solid`].
    pub background: [u8; 3],
}

impl PetConfig {
    pub const DEFAULT: PetConfig = PetConfig {
        coat: Coat::Tuxedo,
        environment: Environment::LivingRoom,
        background: [126, 170, 196],
    };
}

impl Default for PetConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

static CONFIG: Mutex<PetConfig> = Mutex::new(PetConfig::DEFAULT);

pub fn config() -> PetConfig {
    *CONFIG.lock().unwrap()
}

pub fn set_config(cfg: PetConfig) {
    *CONFIG.lock().unwrap() = cfg;
}

/// Parse `#rrggbb` (the leading `#` is optional).
pub fn parse_hex_color(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

pub fn format_hex_color(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

// ─── Sprites ─────────────────────────────────────────────────────────────────
//
// Legend:  . transparent   o outline   b body   w light fur (chest, muzzle,
//          paws)   s tabby stripe   e open eye   c closed eye   p pink

type Sprite = [&'static str; SPRITE as usize];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pose {
    Sit,
    SitBlink,
    SitTail,
    Crouch,
    Loaf,
    SleepA,
    SleepB,
    LickA,
    LickB,
    StretchA,
    StretchB,
    LookUp,
    Swat,
    SwatUp,
}

const SIT: Sprite = [
    "................",
    "..o.....o.......",
    ".opo...opo......",
    ".obbooobbo......",
    ".obsbsbsbo......",
    ".obebbbebo......",
    ".obwwpwwbo......",
    "..owwwwwo.......",
    ".obbwwwbbo......",
    ".obbwwwbbo..o...",
    "obbbwwwbbbo.obo.",
    "obbbwwwbbbo.obo.",
    "obbbbwbbbbo.obo.",
    "obbbbbbbbbooobo.",
    "obbowwowwobbbo..",
    ".ooooooooooooo..",
];

const SIT_BLINK: Sprite = [
    "................",
    "..o.....o.......",
    ".opo...opo......",
    ".obbooobbo......",
    ".obsbsbsbo......",
    ".obcbbbcbo......",
    ".obwwpwwbo......",
    "..owwwwwo.......",
    ".obbwwwbbo......",
    ".obbwwwbbo..o...",
    "obbbwwwbbbo.obo.",
    "obbbwwwbbbo.obo.",
    "obbbbwbbbbo.obo.",
    "obbbbbbbbbooobo.",
    "obbowwowwobbbo..",
    ".ooooooooooooo..",
];

const SIT_TAIL: Sprite = [
    "................",
    "..o.....o.......",
    ".opo...opo......",
    ".obbooobbo......",
    ".obsbsbsbo......",
    ".obebbbebo......",
    ".obwwpwwbo......",
    "..owwwwwo.......",
    ".obbwwwbbo......",
    ".obbwwwbbo....o.",
    "obbbwwwbbbo..obo",
    "obbbwwwbbbo.obo.",
    "obbbbwbbbbo.obo.",
    "obbbbbbbbbooobo.",
    "obbowwowwobbbo..",
    ".ooooooooooooo..",
];

const CROUCH: Sprite = [
    "................",
    "................",
    "................",
    "..o.....o.......",
    ".opo...opo......",
    ".obbooobbo......",
    ".obsbsbsbo......",
    ".obebbbebo......",
    ".obwwpwwbo......",
    "..owwwwwo...o...",
    ".obbwwwbboo.obo.",
    "obbbwwwbbbbo.obo",
    "obbbbwbbbbbooobo",
    "obbbbbbbbbbbbbo.",
    "obwwobbbbowwbo..",
    ".ooooooooooooo..",
];

const LOAF: Sprite = [
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "..o..o..........",
    ".obooboooooo....",
    ".obbbbobbsbsboo.",
    "obebbebobbbbbbbo",
    "obbwpwbbsbsbbbbo",
    "obwwwwbbbbbbbbbo",
    ".obbbbbbbbwwwwbo",
    "..oooooooooooo..",
];

const SLEEP_A: Sprite = [
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "..o..o..........",
    ".obooboooooo....",
    ".obbbbobbsbsboo.",
    "obcbbcbobbbbbbbo",
    "obbwpwbbsbsbbbbo",
    "obwwwwbbbbbbbbbo",
    ".obbbbbbbbwwwwbo",
    "..oooooooooooo..",
];

const SLEEP_B: Sprite = [
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "..o..o..oooo....",
    ".oboobooobbsboo.",
    ".obbbbobbbbbbbbo",
    "obcbbcbbbsbsbbbo",
    "obbwpwbbbbbbbbbo",
    "obwwwwbbsbsbbbbo",
    ".obbbbbbbbwwwwbo",
    "..oooooooooooo..",
];

const LICK_A: Sprite = [
    "................",
    "..o.....o.......",
    ".opo...opo......",
    ".obbooobbo......",
    ".obsbsbsbo......",
    ".obcbbbcbo......",
    ".obwwpwwbo......",
    "..owwpowwo......",
    ".obbwowwwo......",
    ".obbwowwwo..o...",
    "obbbwwoooo..obo.",
    "obbbwwwbbbo.obo.",
    "obbbbwbbbbo.obo.",
    "obbbbbbbbbooobo.",
    "obbowwobbbbbbo..",
    ".ooooooooooooo..",
];

const LICK_B: Sprite = [
    "................",
    "................",
    "..o.....o.......",
    ".opo...opo......",
    ".obbooobbo......",
    ".obsbsbsbo......",
    ".obcbbbcbo......",
    ".obwwpwwbo......",
    "..owwoowwo......",
    ".obbwowwwo..o...",
    "obbbwwoooo..obo.",
    "obbbwwwbbbo.obo.",
    "obbbbwbbbbo.obo.",
    "obbbbbbbbbooobo.",
    "obbowwobbbbbbo..",
    ".ooooooooooooo..",
];

const STRETCH_A: Sprite = [
    "................",
    "..............o.",
    ".............obo",
    ".............obo",
    "...........ooobo",
    "..........obbbbo",
    ".........obsbsbo",
    "....o.o.obbbbbbo",
    "...obobobbsbsbbo",
    "..obbbbbbbbbobbo",
    "..obcbcbbbboobbo",
    "..owwpwbbbo.obbo",
    "...owwwbbo..obbo",
    "ooooowwbo...obbo",
    "owwwwwwo....owwo",
    "oooooooo....oooo",
];

const STRETCH_B: Sprite = [
    "...............o",
    "..............ob",
    ".............obo",
    ".............obo",
    "...........ooobo",
    "..........obbbbo",
    ".........obsbsbo",
    ".........obbbbbo",
    "....o.o.obsbsbbo",
    "...oboboobbbobbo",
    "..obbbbbbbboobbo",
    "..obcbcbbbo.obbo",
    "..owwpwbbo..obbo",
    "oooowwwbo...obbo",
    "owwwwwwo....owwo",
    "oooooooo....oooo",
];

const LOOK_UP: Sprite = [
    "..o.....o.......",
    ".opo...opo......",
    ".obbooobbo......",
    ".obebbbebo......",
    ".obwwpwwbo......",
    ".obwwwwwbo......",
    ".obbwwwbbo......",
    "..obbbbbo.......",
    ".obbwwwbbo......",
    ".obbwwwbbo..o...",
    "obbbwwwbbbo.obo.",
    "obbbwwwbbbo.obo.",
    "obbbbwbbbbo.obo.",
    "obbbbbbbbbooobo.",
    "obbowwowwobbbo..",
    ".ooooooooooooo..",
];

const SWAT: Sprite = [
    "................",
    "..o.....o.......",
    ".opo...opo......",
    ".obbooobbo......",
    ".obsbsbsbo......",
    ".obebbbebo......",
    ".obwwpwwbo......",
    "..owwwwwo.......",
    ".obbwwwbbooooo..",
    ".obbwwwbbbbwwwo.",
    "obbbwwwbbbooooo.",
    "obbbwwwbbbo.....",
    "obbbbwbbbbo.....",
    "obbbbbbbbbo.....",
    "obbowwobbbo.....",
    ".oooooooooo.....",
];

const SWAT_UP: Sprite = [
    "..o.....o.......",
    ".opo...opo......",
    ".obbooobbo......",
    ".obebbbebo......",
    ".obwwpwwbo.ooo..",
    ".obwwwwwbooowwo.",
    ".obbwwwbbobwwwo.",
    "..obbbbbobboooo.",
    ".obbwwwbbbbo....",
    ".obbwwwbbbo.....",
    "obbbwwwbbbo.....",
    "obbbwwwbbbo.....",
    "obbbbwbbbbo.....",
    "obbbbbbbbbo.....",
    "obbowwobbbo.....",
    ".oooooooooo.....",
];

impl Pose {
    fn sprite(self) -> &'static Sprite {
        match self {
            Pose::Sit => &SIT,
            Pose::SitBlink => &SIT_BLINK,
            Pose::SitTail => &SIT_TAIL,
            Pose::Crouch => &CROUCH,
            Pose::Loaf => &LOAF,
            Pose::SleepA => &SLEEP_A,
            Pose::SleepB => &SLEEP_B,
            Pose::LickA => &LICK_A,
            Pose::LickB => &LICK_B,
            Pose::StretchA => &STRETCH_A,
            Pose::StretchB => &STRETCH_B,
            Pose::LookUp => &LOOK_UP,
            Pose::Swat => &SWAT,
            Pose::SwatUp => &SWAT_UP,
        }
    }
}

/// Colour of sprite cell `ch` for `coat`, or `None` for transparent.
fn coat_color(coat: Coat, ch: u8) -> Option<[u8; 3]> {
    // (outline, body, light, stripe, eye)
    let (outline, body, light, stripe, eye) = match coat {
        Coat::Tuxedo => ([14, 14, 20], [46, 46, 56], [244, 244, 240], [46, 46, 56], [150, 220, 110]),
        Coat::Black => ([8, 8, 12], [38, 38, 48], [56, 56, 68], [38, 38, 48], [250, 210, 70]),
        Coat::Orange => ([96, 48, 16], [234, 146, 60], [252, 228, 194], [188, 100, 30], [120, 196, 90]),
    };
    match ch {
        b'o' | b'c' => Some(outline),
        b'b' => Some(body),
        b'w' => Some(light),
        b's' => Some(stripe),
        b'e' => Some(eye),
        b'p' => Some([240, 150, 160]),
        _ => None,
    }
}

// ─── Animation ───────────────────────────────────────────────────────────────

/// A fixed sequence of (pose, duration in steps).
type Script = &'static [(Pose, u32)];

const SLEEP_SCRIPT: Script = &[
    (Pose::Crouch, 8),
    (Pose::Loaf, 24),
    (Pose::SleepA, 32),
    (Pose::SleepB, 32),
    (Pose::SleepA, 32),
    (Pose::SleepB, 32),
    (Pose::SleepA, 32),
    (Pose::SleepB, 32),
    (Pose::SleepA, 32),
    (Pose::SleepB, 32),
    (Pose::SleepA, 32),
    (Pose::Loaf, 24),
    (Pose::Crouch, 8),
];

const LICK_SCRIPT: Script = &[
    (Pose::LickA, 7),
    (Pose::LickB, 7),
    (Pose::LickA, 7),
    (Pose::LickB, 7),
    (Pose::LickA, 7),
    (Pose::LickB, 7),
    (Pose::SitBlink, 14),
    (Pose::LickA, 7),
    (Pose::LickB, 7),
    (Pose::LickA, 7),
    (Pose::LickB, 7),
    (Pose::LickA, 7),
    (Pose::LickB, 7),
];

const STRETCH_SCRIPT: Script = &[
    (Pose::Crouch, 8),
    (Pose::StretchA, 16),
    (Pose::StretchB, 40),
    (Pose::StretchA, 16),
    (Pose::Crouch, 8),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Anim {
    Idle,
    Sleep,
    Lick,
    Stretch,
    Ball,
}

impl Anim {
    fn script(self) -> Option<Script> {
        match self {
            Anim::Sleep => Some(SLEEP_SCRIPT),
            Anim::Lick => Some(LICK_SCRIPT),
            Anim::Stretch => Some(STRETCH_SCRIPT),
            Anim::Idle | Anim::Ball => None,
        }
    }
}

const BALL_RADIUS: f32 = 2.0;
const GRAVITY: f32 = 0.18;
/// Where the cat's paw meets the ball.
const BALL_HOME_X: f32 = (CAT_X + SPRITE) as f32 + 0.5;
/// Gentle upward bats before the ball is knocked away.
const LIGHT_SWATS: u32 = 3;

struct Ball {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

struct PetState {
    anim: Anim,
    /// Steps since the current animation started.
    step: u32,
    idle_len: u32,
    /// Animations still to play this round; refilled and shuffled when empty.
    bag: Vec<Anim>,
    /// Last animation played, so a new round doesn't open with a repeat.
    last_played: Anim,
    pose: Pose,
    prev_pose: Pose,
    /// Steps since the pose last changed.
    since_pose: u32,
    ball: Option<Ball>,
    floor_hits: u32,
    swats: u32,
    swat_timer: u32,
    /// Steps since the ball left the screen.
    ball_gone: u32,
    /// Floating "z"s: (x, y, age).
    zs: Vec<(f32, f32, u32)>,
    /// Steps since start; drives the animated scenery.
    tick: u64,
    rng: u32,
    last: Instant,
    carry: Duration,
}

impl PetState {
    fn new() -> Self {
        Self {
            anim: Anim::Idle,
            step: 0,
            idle_len: 60,
            bag: Vec::new(),
            last_played: Anim::Idle,
            pose: Pose::Sit,
            prev_pose: Pose::Sit,
            since_pose: FADE_STEPS,
            ball: None,
            floor_hits: 0,
            swats: 0,
            swat_timer: 0,
            ball_gone: 0,
            zs: Vec::new(),
            tick: 0,
            rng: 0x0C47_F00D,
            last: Instant::now(),
            carry: Duration::ZERO,
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    fn start(&mut self, anim: Anim) {
        self.anim = anim;
        self.step = 0;
        match anim {
            Anim::Idle => self.idle_len = 60 + (self.rand() * 60.0) as u32,
            Anim::Ball => {
                self.ball = Some(Ball { x: BALL_HOME_X, y: -3.0, vx: 0.0, vy: 0.0 });
                self.floor_hits = 0;
                self.swats = 0;
                self.swat_timer = 0;
                self.ball_gone = 0;
            }
            _ => {}
        }
    }

    fn next_from_bag(&mut self) -> Anim {
        if self.bag.is_empty() {
            self.bag = vec![Anim::Sleep, Anim::Lick, Anim::Stretch, Anim::Ball];
            for i in (1..self.bag.len()).rev() {
                let j = (self.rand() * (i + 1) as f32) as usize;
                self.bag.swap(i, j.min(i));
            }
            if self.bag.last() == Some(&self.last_played) {
                self.bag.swap(0, 3);
            }
        }
        let next = self.bag.pop().unwrap();
        self.last_played = next;
        next
    }

    fn step(&mut self) {
        let pose = match self.anim {
            Anim::Idle => {
                if self.step >= self.idle_len {
                    let next = self.next_from_bag();
                    self.start(next);
                    Pose::Sit
                } else if self.step % 75 >= 71 {
                    Pose::SitBlink
                } else if (self.step / 18) % 4 == 3 {
                    Pose::SitTail
                } else {
                    Pose::Sit
                }
            }
            Anim::Ball => self.step_ball(),
            scripted => {
                let script = scripted.script().unwrap();
                let mut t = self.step;
                let mut pose = None;
                for &(p, len) in script {
                    if t < len {
                        pose = Some(p);
                        break;
                    }
                    t -= len;
                }
                pose.unwrap_or_else(|| {
                    self.start(Anim::Idle);
                    Pose::Sit
                })
            }
        };
        self.step += 1;
        self.tick += 1;

        if pose != self.pose {
            self.prev_pose = self.pose;
            self.pose = pose;
            self.since_pose = 0;
        } else {
            self.since_pose = self.since_pose.saturating_add(1);
        }

        // Sleeping "z"s drift up and to the right.
        if matches!(self.pose, Pose::SleepA | Pose::SleepB) && self.step % 40 == 0 {
            self.zs.push((CAT_X as f32 + 5.0, (FLOOR_Y - 10) as f32, 0));
        }
        for z in &mut self.zs {
            z.0 += 0.07;
            z.1 -= 0.14;
            z.2 += 1;
        }
        self.zs.retain(|z| z.2 < 90);
    }

    fn step_ball(&mut self) -> Pose {
        let floor = FLOOR_Y as f32 - 1.0 - BALL_RADIUS;
        let Some(ball) = self.ball.as_mut() else {
            // Ball is gone: watch it leave for a moment, then carry on.
            self.ball_gone += 1;
            if self.ball_gone > 30 {
                self.start(Anim::Idle);
            }
            return Pose::Sit;
        };

        ball.vy += GRAVITY;
        ball.x += ball.vx;
        ball.y += ball.vy;
        if ball.y > floor {
            ball.y = floor;
            ball.vy = if ball.vy > 0.8 { -ball.vy * 0.6 } else { 0.0 };
            ball.vx *= 0.8;
            self.floor_hits += 1;
        }
        let (x, y, falling) = (ball.x, ball.y, ball.vy >= 0.0);

        if x > GRID as f32 + BALL_RADIUS + 2.0 {
            self.ball = None;
            return Pose::Sit;
        }

        // Let it bounce twice, then bat it whenever it drops back into reach.
        let in_reach = falling && y >= (FLOOR_Y - 12) as f32 && (x - BALL_HOME_X).abs() < 6.0;
        if self.swat_timer == 0 && self.floor_hits >= 2 && self.swats <= LIGHT_SWATS && in_reach {
            let (r1, r2) = (self.rand(), self.rand());
            let ball = self.ball.as_mut().unwrap();
            if self.swats < LIGHT_SWATS {
                ball.vy = -(2.5 + r1 * 0.6);
                ball.vx = (BALL_HOME_X - x) * 0.04 + (r2 - 0.5) * 0.25;
                self.swat_timer = 7;
            } else {
                // Final swipe: off the right-hand edge.
                ball.vy = -2.1;
                ball.vx = 1.9;
                self.swat_timer = 14;
            }
            self.swats += 1;
        }

        if self.swat_timer > 0 {
            self.swat_timer -= 1;
            if self.swats > LIGHT_SWATS { Pose::Swat } else { Pose::SwatUp }
        } else if y < (FLOOR_Y - SPRITE) as f32 && self.swats <= LIGHT_SWATS {
            Pose::LookUp
        } else {
            Pose::Sit
        }
    }
}

static STATE: OnceLock<Mutex<PetState>> = OnceLock::new();

// ─── Rendering ───────────────────────────────────────────────────────────────

/// The 60×60 scene buffer.
struct Canvas {
    px: Vec<[u8; 3]>,
}

impl Canvas {
    fn new(fill: [u8; 3]) -> Self {
        Self { px: vec![fill; (GRID * GRID) as usize] }
    }

    fn get(&self, x: i32, y: i32) -> Option<[u8; 3]> {
        (x >= 0 && y >= 0 && x < GRID && y < GRID).then(|| self.px[(y * GRID + x) as usize])
    }

    fn set(&mut self, x: i32, y: i32, c: [u8; 3]) {
        if x >= 0 && y >= 0 && x < GRID && y < GRID {
            self.px[(y * GRID + x) as usize] = c;
        }
    }

    /// Blend `c` over the existing pixel with opacity `a` (0–1).
    fn blend(&mut self, x: i32, y: i32, c: [u8; 3], a: f32) {
        if let Some(bg) = self.get(x, y) {
            let mix = |i: usize| (bg[i] as f32 + (c[i] as f32 - bg[i] as f32) * a).round() as u8;
            self.set(x, y, [mix(0), mix(1), mix(2)]);
        }
    }

    /// Fill the rectangle with corners (x0, y0) and (x1, y1), inclusive.
    fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: [u8; 3]) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.set(x, y, c);
            }
        }
    }

    fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, c: [u8; 3]) {
        for y in (cy - ry).floor() as i32..=(cy + ry).ceil() as i32 {
            for x in (cx - rx).floor() as i32..=(cx + rx).ceil() as i32 {
                let (dx, dy) = ((x as f32 - cx) / rx, (y as f32 - cy) / ry);
                if dx * dx + dy * dy <= 1.0 {
                    self.set(x, y, c);
                }
            }
        }
    }

    /// Add light: brightens pixels within `r` of (cx, cy), falling off with
    /// distance.  `strength` is the fraction of `color` added at the centre.
    fn glow(&mut self, cx: f32, cy: f32, r: f32, color: [u8; 3], strength: f32) {
        for y in (cy - r).floor() as i32..=(cy + r).ceil() as i32 {
            for x in (cx - r).floor() as i32..=(cx + r).ceil() as i32 {
                let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
                let (Some(px), true) = (self.get(x, y), d < r) else { continue };
                let k = (1.0 - d / r).powi(2) * strength;
                let add = |i: usize| (px[i] as f32 + color[i] as f32 * k).min(255.0) as u8;
                self.set(x, y, [add(0), add(1), add(2)]);
            }
        }
    }

    /// Darken the whole canvas to `factor` of its brightness.
    fn dim(&mut self, factor: f32) {
        for px in &mut self.px {
            *px = px.map(|v| (v as f32 * factor) as u8);
        }
    }

    /// Draw a one-colour shape (`o` cells) with its top-left at (x0, y0),
    /// optionally mirrored left-to-right.
    fn stamp(&mut self, shape: &[&str], x0: i32, y0: i32, color: [u8; 3], mirror: bool) {
        for (row, line) in shape.iter().enumerate() {
            let w = line.len() as i32;
            for (col, ch) in line.bytes().enumerate() {
                if ch == b'o' {
                    let col = if mirror { w - 1 - col as i32 } else { col as i32 };
                    self.set(x0 + col, y0 + row as i32, color);
                }
            }
        }
    }

    /// A pine: stacked triangles with its tip at (cx, top) and base at `base`.
    fn pine(&mut self, cx: i32, top: i32, base: i32, c: [u8; 3]) {
        for y in top..=base {
            let half = ((y - top) % 6 + (y - top) / 6 * 2) / 2 + 1;
            self.rect(cx - half, y, cx + half, y, c);
        }
    }

    /// Draw the cat part-way (`t` in 0–1) through a cross-fade from `from`
    /// to `to`.  Each cell fades between what the two poses show there (the
    /// background where a pose is transparent), so cells that are the same
    /// in both poses stay solid instead of ghosting.
    fn cat(&mut self, from: &Sprite, to: &Sprite, coat: Coat, x0: i32, y0: i32, t: f32) {
        for row in 0..SPRITE {
            for col in 0..SPRITE {
                let (x, y) = (x0 + col, y0 + row);
                let Some(bg) = self.get(x, y) else { continue };
                let cell = |s: &Sprite| s[row as usize].as_bytes()[col as usize];
                let a = coat_color(coat, cell(from)).unwrap_or(bg);
                let b = coat_color(coat, cell(to)).unwrap_or(bg);
                self.set(x, y, a);
                self.blend(x, y, b, t);
            }
        }
    }
}

/// Cheap deterministic hash for scattering scenery details.
fn scatter(x: i32, y: i32) -> u32 {
    let h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263);
    (h ^ (h >> 13)).wrapping_mul(1_274_126_177) >> 16
}

fn paint_forest(c: &mut Canvas) {
    // Sky in flat bands, lightening towards the horizon.
    for y in 0..42 {
        let band = (y / 7) as f32 / 5.0;
        let mix = |a: f32, b: f32| (a + (b - a) * band) as u8;
        c.rect(0, y, GRID - 1, y, [mix(118.0, 196.0), mix(192.0, 232.0), mix(226.0, 214.0)]);
    }
    // Clouds
    c.rect(34, 5, 43, 6, [240, 248, 250]);
    c.rect(36, 4, 41, 4, [240, 248, 250]);
    c.rect(14, 10, 20, 11, [240, 248, 250]);
    // Two rows of pines
    for (i, cx) in (2..GRID).step_by(8).enumerate() {
        c.pine(cx, 18 + (scatter(i as i32, 1) % 6) as i32, 42, [70, 138, 118]);
    }
    for (i, cx) in (-2..GRID + 4).step_by(9).enumerate() {
        c.pine(cx, 22 + (scatter(i as i32, 2) % 8) as i32, 43, [40, 100, 80]);
    }
    // Grass
    c.rect(0, 42, GRID - 1, 43, [64, 132, 66]);
    c.rect(0, 44, GRID - 1, GRID - 1, [96, 168, 84]);
    for y in 45..GRID {
        for x in 0..GRID {
            match scatter(x, y) % 53 {
                0..=3 => c.set(x, y, [74, 146, 72]),
                4 => c.set(x, y, [130, 192, 100]),
                _ => {}
            }
        }
    }
    // Flowers
    for &(x, y, col) in &[
        (6, 54, [250, 220, 90]),
        (13, 48, [244, 244, 240]),
        (44, 56, [240, 150, 180]),
        (52, 49, [250, 220, 90]),
        (18, 57, [240, 150, 180]),
    ] {
        c.set(x, y, col);
        c.set(x, y + 1, [52, 118, 60]);
    }
    // Foreground trunks with canopies
    for &(x0, x1) in &[(2, 7), (51, 56)] {
        c.rect(x0, 0, x1, 46, [110, 76, 50]);
        c.rect(x0, 0, x0 + 1, 46, [84, 56, 38]);
        c.rect(x0 - 1, 45, x1 + 1, 46, [84, 56, 38]);
        for y in (6..44).step_by(9) {
            c.rect(x0 + 3, y, x0 + 4, y + 1, [84, 56, 38]);
        }
    }
    c.ellipse(4.0, 2.0, 12.0, 8.0, [34, 92, 60]);
    c.ellipse(8.0, 6.0, 8.0, 5.0, [48, 116, 70]);
    c.ellipse(55.0, 1.0, 13.0, 9.0, [34, 92, 60]);
    c.ellipse(50.0, 6.0, 8.0, 5.0, [48, 116, 70]);
    // Mushroom
    c.rect(46, 51, 47, 53, [236, 226, 206]);
    c.rect(44, 49, 49, 50, [206, 60, 56]);
    c.rect(45, 48, 48, 48, [206, 60, 56]);
    c.set(45, 49, [244, 244, 240]);
    c.set(48, 50, [244, 244, 240]);
    // Bush
    c.ellipse(11.0, 46.0, 5.0, 3.0, [48, 116, 70]);
    c.set(9, 45, [220, 70, 80]);
    c.set(13, 46, [220, 70, 80]);
}

fn paint_living_room(c: &mut Canvas) {
    // Striped wallpaper
    for x in 0..GRID {
        let col = if x % 6 < 3 { [216, 194, 164] } else { [206, 182, 150] };
        c.rect(x, 0, x, 39, col);
    }
    // Baseboard and wooden floor
    c.rect(0, 40, GRID - 1, 41, [112, 76, 52]);
    c.rect(0, 42, GRID - 1, GRID - 1, [180, 128, 84]);
    for (i, y) in (42..GRID).step_by(6).enumerate() {
        c.rect(0, y, GRID - 1, y, [150, 102, 66]);
        for x in ((i as i32 % 2) * 9..GRID).step_by(18) {
            c.rect(x, y, x, y + 5, [150, 102, 66]);
        }
    }
    // Window with curtains
    c.rect(5, 6, 23, 27, [244, 240, 230]);
    c.rect(7, 8, 21, 25, [142, 204, 238]);
    c.rect(14, 8, 14, 25, [244, 240, 230]);
    c.rect(7, 16, 21, 16, [244, 240, 230]);
    c.rect(16, 10, 19, 11, [250, 252, 252]);
    c.rect(8, 21, 12, 22, [250, 252, 252]);
    c.rect(4, 28, 24, 28, [200, 190, 172]);
    c.rect(3, 4, 25, 5, [112, 76, 52]);
    for &(x0, x1) in &[(3, 6), (22, 25)] {
        c.rect(x0, 6, x1, 31, [196, 84, 84]);
        c.rect(x0, 6, x0, 31, [160, 62, 66]);
    }
    // Picture
    c.rect(42, 8, 54, 19, [202, 162, 64]);
    c.rect(44, 10, 52, 17, [150, 206, 232]);
    c.rect(44, 15, 52, 17, [104, 170, 96]);
    c.rect(49, 11, 50, 12, [250, 230, 120]);
    // Sofa
    c.rect(34, 26, 58, 40, [86, 116, 160]);
    c.rect(34, 26, 58, 27, [112, 144, 188]);
    c.rect(34, 34, 58, 34, [64, 90, 130]);
    c.rect(46, 28, 46, 33, [64, 90, 130]);
    c.rect(32, 30, 35, 40, [72, 100, 142]);
    c.rect(57, 30, 59, 40, [72, 100, 142]);
    c.rect(34, 41, 35, 42, [70, 46, 32]);
    c.rect(56, 41, 57, 42, [70, 46, 32]);
    c.rect(38, 29, 43, 33, [232, 196, 110]);
    // Potted plant
    c.rect(9, 36, 15, 41, [196, 104, 64]);
    c.rect(8, 35, 16, 36, [214, 122, 78]);
    c.ellipse(12.0, 31.0, 4.0, 4.0, [70, 148, 84]);
    c.ellipse(9.0, 33.0, 2.0, 2.0, [54, 124, 70]);
    c.ellipse(15.0, 32.0, 2.0, 3.0, [54, 124, 70]);
    // Rug under the cat
    c.ellipse(30.0, 51.0, 19.0, 5.0, [228, 190, 124]);
    c.ellipse(30.0, 51.0, 17.0, 4.0, [170, 72, 84]);
    c.ellipse(30.0, 51.0, 11.0, 2.0, [196, 98, 104]);
}

/// Fully saturated colour for `hue` in 0–1.
fn hue_rgb(hue: f32) -> [u8; 3] {
    let h = hue.rem_euclid(1.0) * 6.0;
    let x = (1.0 - (h % 2.0 - 1.0).abs()) * 255.0;
    let (r, g, b) = match h as u32 {
        0 => (255.0, x, 0.0),
        1 => (x, 255.0, 0.0),
        2 => (0.0, 255.0, x),
        3 => (0.0, x, 255.0),
        4 => (x, 0.0, 255.0),
        _ => (255.0, 0.0, x),
    };
    [r as u8, g as u8, b as u8]
}

/// Scheduling for occasional scenery events (a bat, the witch, the flying
/// book).  Time is cut into slots of `slot_len` steps; in `chance_pct` percent
/// of them the event plays for the slot's first `duration` steps.  Returns the
/// progress through the event (0–1) and the slot number.
fn event(tick: u64, slot_len: u64, duration: u64, chance_pct: u32, salt: i32) -> Option<(f32, u64)> {
    let (slot, within) = (tick / slot_len, tick % slot_len);
    (within < duration && scatter(slot as i32, salt) % 100 < chance_pct)
        .then(|| (within as f32 / duration as f32, slot))
}

/// (slot length, duration, chance %, salt) for [`event`].
const BAT_EVENT: (u64, u64, u32, i32) = (300, 110, 50, 5);
const WITCH_EVENT: (u64, u64, u32, i32) = (1500, 210, 12, 3);
const BOOK_EVENT: (u64, u64, u32, i32) = (360, 130, 55, 8);

/// Moon centre: in the gap between the two foreground canopies.
const MOON: (f32, f32) = (30.0, 10.0);

/// Witch on a broomstick, flying right (bristles trail on the left).
const WITCH: [&str; 10] = [
    "..........o.....",
    ".........oo.....",
    "........ooo.....",
    "......ooooooo...",
    "........ooo.....",
    ".......oooo.o...",
    "oo...ooooooo....",
    "oooooooooooooooo",
    "ooo....oo.......",
    "oo......o.......",
];
const WITCH_W: f32 = 16.0;

const BAT: [[&str; 4]; 2] = [
    ["o.......o", "oo.o.o.oo", ".ooooooo.", "...ooo..."],
    [".........", "..ooooo..", ".ooooooo.", "oo.ooo.oo"],
];

const SPIDER: [[&str; 3]; 2] = [["o.o.o", ".ooo.", "o.o.o"], ["o...o", "ooooo", "o...o"]];

fn paint_night_forest(c: &mut Canvas, tick: u64) {
    let t = tick as f32;
    // Night sky, a little lighter towards the horizon
    for y in 0..42 {
        let band = (y / 7) as f32 / 5.0;
        let mix = |a: f32, b: f32| (a + (b - a) * band) as u8;
        c.rect(0, y, GRID - 1, y, [mix(10.0, 30.0), mix(14.0, 42.0), mix(38.0, 80.0)]);
    }
    // Twinkling stars
    for i in 0..36 {
        let (x, y) = ((scatter(i, 11) % 60) as i32, (scatter(i, 12) % 30) as i32);
        let twinkle = (t * 0.06 + i as f32 * 1.7).sin() * 0.5 + 0.5;
        c.blend(x, y, [230, 236, 255], 0.25 + 0.75 * twinkle);
    }
    // Full moon
    c.glow(MOON.0, MOON.1, 14.0, [90, 100, 150], 0.55);
    c.ellipse(MOON.0, MOON.1, 6.0, 6.0, [244, 238, 204]);
    for &(dx, dy) in &[(-3, -2), (-2, -2), (2, 1), (2, 2), (-2, 3), (1, -3)] {
        c.set(MOON.0 as i32 + dx, MOON.1 as i32 + dy, [214, 208, 176]);
    }
    // Very rarely: a witch on a broomstick, straight across the moon
    if let Some((p, slot)) = event(tick, WITCH_EVENT.0, WITCH_EVENT.1, WITCH_EVENT.2, WITCH_EVENT.3) {
        let leftward = slot % 2 == 1;
        let travel = -WITCH_W - 2.0 + p * (GRID as f32 + 2.0 * WITCH_W + 4.0);
        let x = if leftward { GRID as f32 - WITCH_W - travel } else { travel };
        let y = MOON.1 - 5.0 + (p * 9.0).sin() * 0.8;
        c.stamp(&WITCH, x.round() as i32, y.round() as i32, [6, 6, 12], leftward);
    }
    // Now and then: a bat flapping across the sky
    if let Some((p, slot)) = event(tick, BAT_EVENT.0, BAT_EVENT.1, BAT_EVENT.2, BAT_EVENT.3) {
        let x = -10.0 + p * (GRID as f32 + 20.0);
        let x = if slot % 2 == 0 { x } else { GRID as f32 - 9.0 - x };
        let y = 13.0 + (scatter(slot as i32, 6) % 8) as f32 + (p * 11.0).sin() * 3.0;
        c.stamp(&BAT[(tick / 4 % 2) as usize], x.round() as i32, y.round() as i32, [8, 8, 16], false);
    }
    // Pine silhouettes
    for (i, cx) in (2..GRID).step_by(8).enumerate() {
        c.pine(cx, 18 + (scatter(i as i32, 1) % 6) as i32, 42, [20, 44, 60]);
    }
    for (i, cx) in (-2..GRID + 4).step_by(9).enumerate() {
        c.pine(cx, 22 + (scatter(i as i32, 2) % 8) as i32, 43, [12, 28, 40]);
    }
    // Grass, with a pool of moonlight where the cat sits
    c.rect(0, 42, GRID - 1, 43, [14, 38, 36]);
    c.rect(0, 44, GRID - 1, GRID - 1, [22, 54, 46]);
    for y in 45..GRID {
        for x in 0..GRID {
            match scatter(x, y) % 53 {
                0..=3 => c.set(x, y, [16, 42, 38]),
                4 => c.set(x, y, [40, 84, 66]),
                _ => {}
            }
        }
    }
    c.glow(CAT_X as f32 + 8.0, FLOOR_Y as f32 + 1.0, 22.0, [60, 90, 100], 0.6);
    // Foreground trunks and canopies
    for &(x0, x1) in &[(2, 7), (51, 56)] {
        c.rect(x0, 0, x1, 46, [34, 28, 40]);
        c.rect(x0, 0, x0 + 1, 46, [22, 18, 28]);
        c.rect(x0 - 1, 45, x1 + 1, 46, [22, 18, 28]);
    }
    c.ellipse(4.0, 2.0, 12.0, 8.0, [10, 26, 30]);
    c.ellipse(8.0, 6.0, 8.0, 5.0, [16, 38, 40]);
    c.ellipse(55.0, 1.0, 13.0, 9.0, [10, 26, 30]);
    c.ellipse(50.0, 6.0, 8.0, 5.0, [16, 38, 40]);
    // Glowing mushrooms, each pulsing at its own pace
    for (i, &(x, y, cap)) in [
        (10, 52, [90, 230, 220]),
        (15, 55, [200, 120, 255]),
        (45, 51, [255, 120, 200]),
        (50, 56, [90, 230, 220]),
        (41, 57, [200, 120, 255]),
        (18, 47, [255, 120, 200]),
    ]
    .iter()
    .enumerate()
    {
        let pulse = (t * 0.05 + i as f32 * 2.1).sin() * 0.5 + 0.5;
        c.glow(x as f32 + 0.5, y as f32 - 1.0, 6.0, cap, 0.25 + 0.45 * pulse);
        c.rect(x, y, x + 1, y + 2, [226, 226, 214]);
        c.rect(x - 1, y - 1, x + 2, y - 1, cap);
        c.rect(x, y - 2, x + 1, y - 2, cap);
        c.set(x - 1, y - 1, [250, 250, 255]);
        c.set(x + 1, y - 2, [250, 250, 255]);
    }
    // Blue fireflies drifting and blinking
    for i in 0..12 {
        let f = i as f32;
        let x = (scatter(i, 21) % 56) as f32 + 2.0 + (t * (0.011 + f * 0.0013) + f).sin() * 5.0;
        let y = (scatter(i, 22) % 26) as f32 + 20.0 + (t * (0.017 + f * 0.0011) + f * 2.3).sin() * 3.0;
        let lit = ((t * (0.045 + f * 0.004) + f * 1.9).sin()).max(0.0);
        if lit > 0.05 {
            c.glow(x, y, 3.5, [70, 150, 255], 0.7 * lit);
            c.blend(x.round() as i32, y.round() as i32, [190, 235, 255], lit);
        }
    }
}

fn paint_dungeon(c: &mut Canvas, tick: u64) {
    let t = tick as f32;
    const MORTAR: [u8; 3] = [38, 36, 50];
    // Stone block wall
    c.rect(0, 0, GRID - 1, 41, [62, 58, 76]);
    for row in 0..7 {
        let y = row * 6;
        for x in (-(row % 2) * 5..GRID).step_by(10) {
            match scatter(x, row) % 9 {
                0 => c.rect(x, y, x + 9, y + 5, [54, 62, 66]), // damp, mossy block
                1 | 2 => c.rect(x, y, x + 9, y + 5, [70, 66, 86]),
                _ => {}
            }
            c.rect(x, y, x, y + 5, MORTAR);
        }
        c.rect(0, y, GRID - 1, y, MORTAR);
    }
    // Flagstone floor
    c.rect(0, 42, GRID - 1, GRID - 1, [80, 76, 92]);
    for (i, y) in (42..GRID).step_by(6).enumerate() {
        c.rect(0, y, GRID - 1, y, MORTAR);
        for x in ((i as i32 % 2) * 7..GRID).step_by(14) {
            c.rect(x, y, x, y + 5, MORTAR);
        }
    }
    // Cobweb in the corner
    for i in 0..9 {
        c.set(i, 0, [150, 150, 164]);
        c.set(0, i, [150, 150, 164]);
        c.set(i, i, [150, 150, 164]);
    }
    for &(x, y) in &[(3, 1), (1, 3), (5, 2), (2, 5), (6, 4), (4, 6), (5, 3), (3, 5)] {
        c.set(x, y, [120, 120, 136]);
    }
    // Shelf of potion bottles and a skull
    c.rect(34, 17, 58, 18, [84, 58, 40]);
    for (i, &x) in [36, 40, 44, 53].iter().enumerate() {
        let liquid = hue_rgb(0.13 + i as f32 * 0.27);
        c.rect(x, 12, x + 2, 16, [150, 170, 180]);
        c.rect(x, 14, x + 2, 16, liquid);
        c.rect(x + 1, 10, x + 1, 11, [150, 170, 180]);
        c.set(x + 1, 9, [140, 100, 70]);
    }
    c.rect(47, 13, 51, 16, [224, 220, 204]);
    c.rect(48, 12, 50, 12, [224, 220, 204]);
    c.set(48, 14, [30, 28, 36]);
    c.set(50, 14, [30, 28, 36]);
    c.set(49, 16, [30, 28, 36]);
    // Wall torch
    c.rect(11, 22, 12, 28, [84, 58, 40]);
    c.rect(10, 21, 13, 21, [110, 110, 120]);
    let flick = (scatter((tick / 3) as i32, 31) % 3) as i32 - 1;
    c.rect(10, 18, 13, 20, [255, 150, 40]);
    c.rect(11, 16, 12, 19, [255, 214, 90]);
    c.set(11 + flick.max(0), 15, [255, 170, 60]);
    c.glow(11.5, 18.0, 14.0, [255, 150, 60], 0.45 + 0.1 * (t * 0.4).sin());

    // A spider letting itself down on a thread, and one pacing the wall
    let legs = (tick / 6 % 2) as usize;
    let drop = (14.0 + (t * 0.02).sin() * 9.0) as i32;
    c.rect(24, 0, 24, drop, [150, 150, 164]);
    c.stamp(&SPIDER[legs], 22, drop, [18, 14, 22], false);
    let crawl = (30.0 + (t * 0.012).sin() * 24.0) as i32;
    c.stamp(&SPIDER[legs], crawl, 33 + (scatter(crawl / 6, 40) % 2) as i32, [18, 14, 22], false);

    // Cauldron over a fire; the brew slowly cycles through the rainbow
    let brew = hue_rgb(t / 480.0);
    let (cx, top) = (50.0, 37);
    for x in 44..=56 {
        let h = scatter(x, (tick / 3) as i32) % 4;
        c.rect(x, 51 - h as i32, x, 52, if h > 2 { [255, 220, 100] } else { [255, 130, 40] });
    }
    c.rect(43, 52, 57, 53, [70, 46, 32]);
    c.glow(cx, 51.0, 9.0, [255, 140, 50], 0.5);
    c.ellipse(cx, 44.0, 8.0, 6.5, [26, 24, 32]);
    c.ellipse(cx - 2.0, 43.0, 4.0, 3.0, [40, 38, 48]);
    c.rect(41, top, 59, top + 1, [44, 42, 54]);
    c.rect(43, top, 57, top, brew);
    c.rect(44, top + 1, 56, top + 1, brew.map(|v| (v as f32 * 0.7) as u8));
    c.glow(cx, top as f32 - 1.0, 12.0, brew, 0.35);
    // Bubbles swell on the surface, rise and pop
    for i in 0..6 {
        let phase = (tick + i * 13) % 44;
        let round = ((tick + i * 13) / 44) as i32;
        let x = 44 + (scatter(i as i32, round) % 12) as i32;
        let y = top - (phase / 4) as i32;
        let pale = brew.map(|v| v / 2 + 127);
        if phase < 36 {
            c.set(x, y, pale);
        } else {
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1)] {
                c.blend(x + dx, y + dy, pale, 0.7);
            }
        }
    }
}

fn paint_library(c: &mut Canvas, tick: u64) {
    let t = tick as f32;
    const SPINES: [[u8; 3]; 8] = [
        [150, 50, 50], [50, 90, 140], [60, 120, 80], [160, 130, 60],
        [110, 70, 130], [150, 90, 50], [70, 70, 90], [190, 180, 150],
    ];
    // Wall of bookshelves
    c.rect(0, 0, GRID - 1, 41, [40, 28, 22]);
    for shelf in 0..5 {
        let y0 = 1 + shelf * 8;
        let mut x = 1;
        while x < GRID - 1 {
            let w = 1 + (scatter(x, shelf) % 2) as i32;
            if scatter(x, shelf + 30) % 11 != 0 {
                let h = 4 + (scatter(x, shelf + 9) % 3) as i32;
                let col = SPINES[(scatter(x, shelf + 20) % 8) as usize];
                c.rect(x, y0 + 7 - h, x + w - 1, y0 + 6, col);
                if w == 2 { c.set(x, y0 + 8 - h, col.map(|v| v / 2 + 110)); }
            }
            x += w;
        }
        c.rect(0, y0 + 7, GRID - 1, y0 + 7, [84, 58, 40]);
    }
    for x in [0, 20, 39, GRID - 1] {
        c.rect(x, 0, x, 41, [70, 48, 34]);
    }
    // Floor boards and a carpet runner
    c.rect(0, 42, GRID - 1, GRID - 1, [70, 50, 40]);
    for y in (42..GRID).step_by(6) {
        c.rect(0, y, GRID - 1, y, [50, 34, 28]);
    }
    c.rect(4, 47, 55, 56, [120, 40, 48]);
    c.rect(4, 47, 55, 47, [190, 150, 80]);
    c.rect(4, 56, 55, 56, [190, 150, 80]);
    for x in (6..54).step_by(4) {
        c.set(x, 51, [190, 150, 80]);
        c.set(x + 1, 52, [190, 150, 80]);
    }
    // Reading desk with an open book
    c.rect(43, 34, 59, 35, [104, 70, 46]);
    c.rect(44, 36, 45, 43, [84, 56, 38]);
    c.rect(57, 36, 58, 43, [84, 56, 38]);
    c.rect(46, 32, 52, 33, [236, 228, 204]);
    c.rect(49, 32, 49, 33, [150, 130, 100]);
    // Candles: a floor stand, one on the desk, one up on a shelf
    c.rect(8, 31, 8, 45, [150, 130, 70]);
    c.rect(6, 45, 10, 46, [150, 130, 70]);
    c.rect(6, 30, 10, 30, [150, 130, 70]);
    const CANDLES: [(i32, i32); 3] = [(8, 29), (55, 33), (30, 15)];

    // It is a *dark* library: everything is lit by the candles alone.
    c.dim(0.42);
    for (i, &(x, base)) in CANDLES.iter().enumerate() {
        let f = i as f32;
        let flicker = 0.78
            + 0.12 * (t * 0.31 + f * 2.0).sin()
            + 0.10 * (t * 0.83 + f * 5.0).sin();
        let gust = scatter((tick / 5) as i32, 50 + i as i32) % 6 == 0;
        c.glow(x as f32, base as f32 - 4.0, 20.0, [255, 170, 80], 0.75 * flicker);
        c.rect(x, base - 3, x, base, [236, 228, 204]);
        c.set(x, base - 4, [255, 170, 60]);
        c.set(x + gust as i32, base - 5, [255, 236, 150]);
    }

    // Now and then a book flaps past, swooping low through the room
    if let Some((p, slot)) = event(tick, BOOK_EVENT.0, BOOK_EVENT.1, BOOK_EVENT.2, BOOK_EVENT.3) {
        let x = -9.0 + p * (GRID as f32 + 18.0);
        let x = if slot % 2 == 0 { x } else { GRID as f32 - 7.0 - x };
        let y = 8.0 + (p * std::f32::consts::PI).sin() * 16.0 + (p * 14.0).sin() * 1.5;
        let (x, y) = (x.round() as i32, y.round() as i32);
        let cover = SPINES[(slot % 6) as usize];
        let up = tick / 5 % 2 == 0;
        // Covers and pages beat like wings around the spine.
        for dx in 0..7i32 {
            let lift = (dx - 3).abs();
            let wing = if up { 2 - lift.min(2) } else { lift.min(2) };
            c.set(x + dx, y + wing, cover);
            if dx != 3 { c.set(x + dx, y + wing + 1, [240, 232, 208]); }
        }
        c.set(x + 3, y + if up { 3 } else { 1 }, cover);
    }
}

fn paint_ball(c: &mut Canvas, ball: &Ball) {
    const BALL: [&str; 5] = [".ooo.", "ohrro", "orrro", "orrdo", ".ooo."];
    let (x0, y0) = ((ball.x - BALL_RADIUS).round() as i32, (ball.y - BALL_RADIUS).round() as i32);
    for (row, line) in BALL.iter().enumerate() {
        for (col, ch) in line.bytes().enumerate() {
            let color = match ch {
                b'o' => [96, 20, 30],
                b'r' => [226, 62, 72],
                b'h' => [252, 190, 190],
                b'd' => [170, 36, 52],
                _ => continue,
            };
            c.set(x0 + col as i32, y0 + row as i32, color);
        }
    }
}

fn paint_scene(st: &PetState, cfg: PetConfig) -> Canvas {
    let mut c = Canvas::new(cfg.background);
    match cfg.environment {
        Environment::Solid => {}
        Environment::Forest => paint_forest(&mut c),
        Environment::LivingRoom => paint_living_room(&mut c),
        Environment::NightForest => paint_night_forest(&mut c, st.tick),
        Environment::Dungeon => paint_dungeon(&mut c, st.tick),
        Environment::Library => paint_library(&mut c, st.tick),
    }

    // Soft ground shadows
    let cat_cx = CAT_X as f32 + 7.0;
    for x in 0..GRID {
        let dx = (x as f32 - cat_cx) / 9.0;
        if dx * dx <= 1.0 {
            c.blend(x, FLOOR_Y, [0, 0, 0], 0.28);
        }
    }
    if let Some(ball) = &st.ball {
        for x in (ball.x - 1.5).round() as i32..=(ball.x + 1.5).round() as i32 {
            c.blend(x, FLOOR_Y, [0, 0, 0], 0.22);
        }
    }

    // Cat: cross-fade from the previous pose to the current one.
    let t = ((st.since_pose + 1) as f32 / FADE_STEPS as f32).min(1.0);
    c.cat(st.prev_pose.sprite(), st.pose.sprite(), cfg.coat, CAT_X, FLOOR_Y - SPRITE, t);

    if let Some(ball) = &st.ball {
        paint_ball(&mut c, ball);
    }

    for &(x, y, age) in &st.zs {
        let a = if age < 10 { age as f32 / 10.0 } else { ((90 - age) as f32 / 40.0).min(1.0) };
        let (x, y) = (x.round() as i32, y.round() as i32);
        for (dx, dy) in [(0, 0), (1, 0), (2, 0), (1, 1), (0, 2), (1, 2), (2, 2)] {
            c.blend(x + dx, y + dy, [250, 250, 255], a * 0.9);
        }
    }
    c
}

/// Advance the simulation to now and render the current frame.
pub fn render_pet_cat() -> DynamicImage {
    let mut guard = STATE.get_or_init(|| Mutex::new(PetState::new())).lock().unwrap();
    let st = &mut *guard;

    // Run however many 30 Hz steps have elapsed (capped, so a long pause in
    // rendering doesn't fast-forward through whole animations).
    let now = Instant::now();
    st.carry += now.duration_since(st.last);
    st.last = now;
    let mut steps = 0;
    while st.carry >= STEP && steps < 6 {
        st.carry -= STEP;
        st.step();
        steps += 1;
    }
    if st.carry >= STEP {
        st.carry = Duration::ZERO;
    }

    let canvas = paint_scene(st, config());
    let mut img = RgbImage::new(GRID as u32 * SCALE, GRID as u32 * SCALE);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgb(canvas.px[((y / SCALE) * GRID as u32 + x / SCALE) as usize]);
    }
    DynamicImage::ImageRgb8(img)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_POSES: [Pose; 14] = [
        Pose::Sit, Pose::SitBlink, Pose::SitTail, Pose::Crouch, Pose::Loaf,
        Pose::SleepA, Pose::SleepB, Pose::LickA, Pose::LickB, Pose::StretchA,
        Pose::StretchB, Pose::LookUp, Pose::Swat, Pose::SwatUp,
    ];

    #[test]
    fn sprites_are_16x16_with_known_cells() {
        for pose in ALL_POSES {
            for (row, line) in pose.sprite().iter().enumerate() {
                assert_eq!(line.len(), 16, "{pose:?} row {row}");
                assert!(
                    line.bytes().all(|b| b".obwsecp".contains(&b)),
                    "{pose:?} row {row}: {line}"
                );
            }
        }
    }

    #[test]
    fn cycles_through_every_animation_and_returns_to_idle() {
        let mut st = PetState::new();
        let mut seen = Vec::new();
        let mut order = Vec::new();
        for _ in 0..30 * 240 {
            st.step();
            if !seen.contains(&st.anim) {
                seen.push(st.anim);
            }
            if st.anim != Anim::Idle && order.last() != Some(&st.anim) {
                order.push(st.anim);
            }
            if st.anim == Anim::Idle && order.last() != Some(&Anim::Idle) {
                order.push(Anim::Idle);
            }
        }
        // Idle separates animations; the same one never plays twice in a row.
        let played: Vec<Anim> = order.into_iter().filter(|a| *a != Anim::Idle).collect();
        assert!(played.len() > 8);
        assert!(played.windows(2).all(|w| w[0] != w[1]), "{played:?}");
        for anim in [Anim::Idle, Anim::Sleep, Anim::Lick, Anim::Stretch, Anim::Ball] {
            assert!(seen.contains(&anim), "{anim:?} never played");
        }
    }

    #[test]
    fn ball_is_batted_then_leaves_the_screen() {
        let mut st = PetState::new();
        st.start(Anim::Ball);
        let mut steps = 0;
        while st.anim == Anim::Ball {
            st.step();
            steps += 1;
            assert!(steps < 30 * 30, "ball animation never finished");
        }
        assert!(st.ball.is_none());
        assert_eq!(st.swats, LIGHT_SWATS + 1);
    }

    #[test]
    fn hex_colors_round_trip() {
        assert_eq!(parse_hex_color("#7eaac4"), Some([126, 170, 196]));
        assert_eq!(parse_hex_color("7EAAC4"), Some([126, 170, 196]));
        assert_eq!(parse_hex_color("#12345"), None);
        assert_eq!(parse_hex_color("#gg0000"), None);
        assert_eq!(format_hex_color([126, 170, 196]), "#7eaac4");
    }

    #[test]
    fn renders_a_panel_sized_frame_for_every_setting() {
        for coat in Coat::ALL {
            for environment in Environment::ALL {
                set_config(PetConfig { coat, environment, ..PetConfig::DEFAULT });
                let img = render_pet_cat();
                assert_eq!((img.width(), img.height()), (480, 480));
            }
        }
        set_config(PetConfig::DEFAULT);
    }

    #[test]
    fn scenery_events_do_happen_and_do_not_overlap_their_slot() {
        for (name, (slot, duration, chance, salt)) in
            [("bat", BAT_EVENT), ("witch", WITCH_EVENT), ("book", BOOK_EVENT)]
        {
            assert!(duration < slot, "{name}");
            // Within an hour of animation each event plays at least once…
            let plays = (0..30 * 3600).step_by(slot as usize)
                .filter(|&t| event(t, slot, duration, chance, salt).is_some())
                .count();
            assert!(plays > 0, "{name} never plays");
            // …and the witch stays rare next to the bat.
            if name == "witch" { assert!(plays < 20, "witch plays {plays} times an hour"); }
        }
    }

    #[test]
    fn witch_passes_in_front_of_the_moon() {
        let (slot, duration, chance, salt) = WITCH_EVENT;
        let start = (0..).step_by(slot as usize)
            .find(|&t| event(t, slot, duration, chance, salt).is_some())
            .unwrap();
        let moon_covered = (start..start + duration).any(|tick| {
            let mut c = Canvas::new([0, 0, 0]);
            paint_night_forest(&mut c, tick);
            c.get(MOON.0 as i32, MOON.1 as i32) == Some([6, 6, 12])
        });
        assert!(moon_covered);
    }
}
