//! Pixel-art pet cat.
//!
//! The scene is painted on a 60×60 grid and scaled ×8 to the 480×480 panel,
//! so every "pixel" on screen is an 8×8 block.  The cat is a 16×16 sprite that
//! cycles through idle, sleeping, paw-licking, stretching and ball-playing
//! animations; every animation starts and ends in the sitting pose, and pose
//! changes are cross-faded over a few frames.
//!
//! The background is one of six environments.  Three are static (solid
//! colour, forest, living room); three are animated (night forest, alchemy
//! dungeon, dark library), with occasional events such as a bat or a flying
//! book scheduled deterministically from the simulation clock.
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
//          paws)   s tabby stripe   e open eye   h half-closed eye
//          c closed eye   p pink
//
// Most sprites face the viewer. The running pair faces right; the scene
// mirrors them when the cat heads left.

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
    SitTail2,
    SitHalf,
    SitEar,
    KneadA,
    KneadB,
    Tilt,
    Yawn,
    Puffed,
    GroomA,
    GroomB,
    RunA,
    RunB,
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

/// Tail tip flicked the other way from `SIT_TAIL`.
const SIT_TAIL2: Sprite = [
    "................",
    "..o.....o.......",
    ".opo...opo......",
    ".obbooobbo......",
    ".obsbsbsbo......",
    ".obebbbebo......",
    ".obwwpwwbo......",
    "..owwwwwo.......",
    ".obbwwwbbo......",
    ".obbwwwbbo.o....",
    "obbbwwwbbboobo..",
    "obbbwwwbbbo.obo.",
    "obbbbwbbbbo.obo.",
    "obbbbbbbbbooobo.",
    "obbowwowwobbbo..",
    ".ooooooooooooo..",
];

/// Eyes half closed, between `SIT` and `SIT_BLINK`.
const SIT_HALF: Sprite = [
    "................",
    "..o.....o.......",
    ".opo...opo......",
    ".obbooobbo......",
    ".obsbsbsbo......",
    ".obhbbbhbo......",
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

/// One ear turned outwards, listening.
const SIT_EAR: Sprite = [
    "................",
    "..o......o......",
    ".opo...obo......",
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

/// Kneading, left paw up.
const KNEAD_A: Sprite = [
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
    "obbowwobbbooobo.",
    "obbbooowwobbbo..",
    ".ooooooooooooo..",
];

/// Kneading, right paw up.
const KNEAD_B: Sprite = [
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
    "obbbbbowwoooobo.",
    "obbowwooobbbbo..",
    ".ooooooooooooo..",
];

/// Head leaning to one side.
const TILT: Sprite = [
    "................",
    "...o.....o......",
    "..opo...opo.....",
    "..obbooobbo.....",
    "..obsbsbsbo.....",
    "..obebbbebo.....",
    "..obwwpwwbo.....",
    "...owwwwwo......",
    ".obbwwwbbo......",
    ".obbwwwbbo..o...",
    "obbbwwwbbbo.obo.",
    "obbbwwwbbbo.obo.",
    "obbbbwbbbbo.obo.",
    "obbbbbbbbbooobo.",
    "obbowwowwobbbo..",
    ".ooooooooooooo..",
];

/// Mid-yawn.
const YAWN: Sprite = [
    "................",
    "..o.....o.......",
    ".opo...opo......",
    ".obbooobbo......",
    ".obsbsbsbo......",
    ".obcbbbcbo......",
    ".obwwpwwbo......",
    "..owpppwo.......",
    ".obbwpwbbo......",
    ".obbwwwbbo..o...",
    "obbbwwwbbbo.obo.",
    "obbbwwwbbbo.obo.",
    "obbbbwbbbbo.obo.",
    "obbbbbbbbbooobo.",
    "obbowwowwobbbo..",
    ".ooooooooooooo..",
];

/// Startled: tail fluffed up.
const PUFFED: Sprite = [
    "................",
    "..o.....o.......",
    ".opo...opo......",
    ".obbooobbo......",
    ".obsbsbsbo......",
    ".obebbbebo......",
    ".obwwpwwbo......",
    "..owwwwwo.......",
    ".obbwwwbbo.ooo..",
    ".obbwwwbbooobbbo",
    "obbbwwwbbboobbbo",
    "obbbwwwbbbo.obbo",
    "obbbbwbbbbo.obo.",
    "obbbbbbbbbooobo.",
    "obbowwowwobbbo..",
    ".ooooooooooooo..",
];

/// Twisted round to lick its flank.
const GROOM_A: Sprite = [
    "................",
    "................",
    "................",
    "....oooo........",
    "...obbbbo.o.o...",
    "..obbsbsbobobo..",
    "..obbbbbbobbbbo.",
    ".obbsbsbbocbwbo.",
    ".obbbbbbbbowpwo.",
    "obbbbbbbbbbooo..",
    "obbsbsbbbbbbo...",
    "obbbbbbbbbbbbo..",
    "obbbbwbbbbbbbo..",
    "obbbbbbbbbbbbo..",
    "obbowwowwobbbo..",
    ".ooooooooooooo..",
];

/// The same, head a little lower.
const GROOM_B: Sprite = [
    "................",
    "................",
    "................",
    "....oooo........",
    "...obbbbo.......",
    "..obbsbsbo.o.o..",
    "..obbbbbbobobo..",
    ".obbsbsbbobbbbo.",
    ".obbbbbbbbocbwbo",
    "obbbbbbbbbbowpwo",
    "obbsbsbbbbbbooo.",
    "obbbbbbbbbbbbo..",
    "obbbbwbbbbbbbo..",
    "obbbbbbbbbbbbo..",
    "obbowwowwobbbo..",
    ".ooooooooooooo..",
];

/// Running to the right, legs stretched out.
const RUN_A: Sprite = [
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "..o.........o.o.",
    ".obo.......obobo",
    ".obooooooooobbbo",
    "..obbsbsbsbbbebo",
    "..obbbbbbbbbbwpo",
    "..obbbbbbbbbwwo.",
    "..obooooooooobo.",
    ".obo........obo.",
    "oo...........oo.",
];

/// Running to the right, legs gathered.
const RUN_B: Sprite = [
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "..o.........o.o.",
    ".obo.......obobo",
    ".obooooooooobbbo",
    "..obbsbsbsbbbebo",
    "..obbbbbbbbbbwpo",
    "..obbbbbbbbbwwo.",
    "..obbooooooobbo.",
    "...obo....obo...",
    "....oo....oo....",
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
            Pose::SitTail2 => &SIT_TAIL2,
            Pose::SitHalf => &SIT_HALF,
            Pose::SitEar => &SIT_EAR,
            Pose::KneadA => &KNEAD_A,
            Pose::KneadB => &KNEAD_B,
            Pose::Tilt => &TILT,
            Pose::Yawn => &YAWN,
            Pose::Puffed => &PUFFED,
            Pose::GroomA => &GROOM_A,
            Pose::GroomB => &GROOM_B,
            Pose::RunA => &RUN_A,
            Pose::RunB => &RUN_B,
        }
    }
}

/// Colour of sprite cell `ch` for `coat`, or `None` for transparent.
fn coat_color(coat: Coat, ch: u8) -> Option<[u8; 3]> {
    // (outline, body, light, stripe, eye)
    let (outline, body, light, stripe, eye) = match coat {
        Coat::Tuxedo => (
            [14, 14, 20],
            [46, 46, 56],
            [244, 244, 240],
            [46, 46, 56],
            [150, 220, 110],
        ),
        Coat::Black => (
            [8, 8, 12],
            [38, 38, 48],
            [56, 56, 68],
            [38, 38, 48],
            [250, 210, 70],
        ),
        Coat::Orange => (
            [96, 48, 16],
            [234, 146, 60],
            [252, 228, 194],
            [188, 100, 30],
            [120, 196, 90],
        ),
    };
    match ch {
        b'o' | b'c' => Some(outline),
        b'b' => Some(body),
        b'w' => Some(light),
        b's' => Some(stripe),
        b'e' => Some(eye),
        // A half-closed eye: the lid halfway down over the eye colour.
        b'h' => Some([0, 1, 2].map(|i| ((u16::from(eye[i]) + u16::from(outline[i])) / 2) as u8)),
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

/// A slow swish each way, then a burst of quick twitches.
const TAIL_FLICK_SCRIPT: Script = &[
    (Pose::SitTail, 16),
    (Pose::Sit, 14),
    (Pose::SitTail2, 16),
    (Pose::Sit, 10),
    (Pose::SitTail, 3),
    (Pose::SitTail2, 3),
    (Pose::SitTail, 3),
    (Pose::SitTail2, 3),
    (Pose::SitTail, 3),
    (Pose::Sit, 16),
];

const SLOW_BLINK_SCRIPT: Script = &[
    (Pose::SitHalf, 10),
    (Pose::SitBlink, 26),
    (Pose::SitHalf, 10),
    (Pose::Sit, 14),
    (Pose::SitHalf, 8),
    (Pose::SitBlink, 18),
    (Pose::SitHalf, 8),
];

const EAR_SWIVEL_SCRIPT: Script = &[
    (Pose::SitEar, 22),
    (Pose::Sit, 10),
    (Pose::SitEar, 12),
    (Pose::Sit, 6),
    (Pose::SitEar, 18),
];

/// Settles into a loaf and dozes without quite falling asleep.
const LOAF_SCRIPT: Script = &[
    (Pose::Crouch, 8),
    (Pose::Loaf, 70),
    (Pose::SleepA, 24),
    (Pose::Loaf, 60),
    (Pose::SleepA, 8),
    (Pose::Loaf, 40),
    (Pose::Crouch, 8),
];

const KNEAD_SCRIPT: Script = &[
    (Pose::KneadA, 7),
    (Pose::KneadB, 7),
    (Pose::KneadA, 7),
    (Pose::KneadB, 7),
    (Pose::KneadA, 7),
    (Pose::KneadB, 7),
    (Pose::KneadA, 7),
    (Pose::KneadB, 7),
    (Pose::KneadA, 7),
    (Pose::KneadB, 7),
    (Pose::KneadA, 7),
    (Pose::KneadB, 7),
    (Pose::SitBlink, 12),
];

const YAWN_SCRIPT: Script = &[
    (Pose::SitHalf, 8),
    (Pose::SitBlink, 6),
    (Pose::Yawn, 26),
    (Pose::SitBlink, 8),
    (Pose::SitHalf, 8),
];

const GROOM_SCRIPT: Script = &[
    (Pose::Crouch, 6),
    (Pose::GroomA, 8),
    (Pose::GroomB, 8),
    (Pose::GroomA, 8),
    (Pose::GroomB, 8),
    (Pose::GroomA, 8),
    (Pose::GroomB, 8),
    (Pose::GroomA, 8),
    (Pose::GroomB, 8),
    (Pose::Crouch, 6),
];

/// Night forest only: gazes up at the fireflies.
const WATCH_SCRIPT: Script = &[
    (Pose::LookUp, 40),
    (Pose::Tilt, 26),
    (Pose::LookUp, 34),
    (Pose::SitBlink, 6),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Anim {
    Idle,
    Sleep,
    Lick,
    Stretch,
    Ball,
    TailFlick,
    SlowBlink,
    EarSwivel,
    Loaf,
    Knead,
    Yawn,
    Groom,
    /// Follows a moth across the room with its eyes.
    Stare,
    /// Tears off one side of the screen and back in from the other.
    Zoomies,
    /// Crouch, wiggle, leap. In the dungeon it is after a spider and in the
    /// forest a butterfly; elsewhere it pounces on nothing.
    Pounce,
    /// Living room only: a box arrives and the cat sits in it.
    Box,
    /// Night forest only.
    Watch,
}

impl Anim {
    /// Animations every environment has, besides idling.
    const COMMON: [Anim; 14] = [
        Anim::Sleep,
        Anim::Lick,
        Anim::Stretch,
        Anim::Ball,
        Anim::TailFlick,
        Anim::SlowBlink,
        Anim::EarSwivel,
        Anim::Loaf,
        Anim::Knead,
        Anim::Yawn,
        Anim::Groom,
        Anim::Stare,
        Anim::Zoomies,
        Anim::Pounce,
    ];

    /// The animations that play in `env`.
    fn repertoire(env: Environment) -> Vec<Anim> {
        let mut anims = Self::COMMON.to_vec();
        match env {
            Environment::LivingRoom => anims.push(Anim::Box),
            Environment::NightForest => anims.push(Anim::Watch),
            _ => {}
        }
        anims
    }

    /// The pose sequence, for animations that are just that.
    fn script(self) -> Option<Script> {
        match self {
            Anim::Sleep => Some(SLEEP_SCRIPT),
            Anim::Lick => Some(LICK_SCRIPT),
            Anim::Stretch => Some(STRETCH_SCRIPT),
            Anim::TailFlick => Some(TAIL_FLICK_SCRIPT),
            Anim::SlowBlink => Some(SLOW_BLINK_SCRIPT),
            Anim::EarSwivel => Some(EAR_SWIVEL_SCRIPT),
            Anim::Loaf => Some(LOAF_SCRIPT),
            Anim::Knead => Some(KNEAD_SCRIPT),
            Anim::Yawn => Some(YAWN_SCRIPT),
            Anim::Groom => Some(GROOM_SCRIPT),
            Anim::Watch => Some(WATCH_SCRIPT),
            Anim::Idle | Anim::Ball | Anim::Stare | Anim::Zoomies | Anim::Pounce | Anim::Box => {
                None
            }
        }
    }
}

/// Something the cat interacts with during an animation.
#[derive(Clone, Copy, PartialEq, Debug)]
enum PropKind {
    Moth,
    Butterfly,
    Spider,
    Box,
}

/// A prop and the position of its top-left corner.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Prop {
    kind: PropKind,
    x: f32,
    y: f32,
}

/// Just past the right-hand edge, where props enter and leave.
const OFFSTAGE_X: f32 = GRID as f32 + 2.0;

// Zoomies
/// Steps spent crouched and twitching before the dash.
const ZOOM_WIND_UP: u32 = 12;
const ZOOM_SPEED: f32 = 2.4;
const ZOOM_LAPS: f32 = 2.0;
/// Distance from the cat's spot to fully off either edge.
const ZOOM_EDGE: f32 = (GRID - CAT_X + 2) as f32;
const ZOOM_SKID: u32 = 8;

// Pounce
const POUNCE_STALK: u32 = 24;
const POUNCE_WIGGLE: u32 = 20;
const POUNCE_LEAP: u32 = 12;
const POUNCE_LAND: u32 = 14;
/// How far the leap carries the cat, and how high.
const POUNCE_DIST: f32 = 12.0;
const POUNCE_HEIGHT: f32 = 6.0;
/// Width allowed for the prey when placing it.
const PREY_W: f32 = 5.0;

/// For a pounce in direction `dir` (+1 right, -1 left): where the prey
/// waits, just beyond where the leap lands, and the off-screen point it
/// comes from and flees to.
fn prey_track(dir: f32) -> (f32, f32) {
    if dir > 0.0 {
        ((CAT_X + SPRITE) as f32 + POUNCE_DIST - 2.0, OFFSTAGE_X)
    } else {
        (CAT_X as f32 - POUNCE_DIST - PREY_W + 2.0, -PREY_W - 2.0)
    }
}

// Box
const BOX_SLIDE: u32 = 20;
const BOX_NOTICE: u32 = 24;
/// How far the cat walks to reach the box.
const BOX_WALK: u32 = 20;
const BOX_SIT: u32 = 150;
const BOX_W: i32 = SPRITE;
const BOX_H: i32 = 7;

// Stare
const STARE_STEPS: u32 = 110;

/// The two running frames, alternating.
fn run_pose(step: u32) -> Pose {
    if (step / 3).is_multiple_of(2) {
        Pose::RunA
    } else {
        Pose::RunB
    }
}

/// Top-left of the library's flying book, `p` (0–1) of the way through its
/// flight. Odd-numbered flights go right to left.
fn book_position(p: f32, slot: u64) -> (f32, f32) {
    let x = -9.0 + p * (GRID as f32 + 18.0);
    let x = if slot.is_multiple_of(2) {
        x
    } else {
        GRID as f32 - 7.0 - x
    };
    let y = 8.0 + (p * std::f32::consts::PI).sin() * 16.0 + (p * 14.0).sin() * 1.5;
    (x, y)
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
    /// The scene the cat is in; decides which animations it has.
    env: Environment,
    /// How far the cat is from its usual spot, in cells.
    dx: f32,
    dy: f32,
    /// Facing left: the sprite is mirrored.
    flip: bool,
    prop: Option<Prop>,
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
            env: Environment::Solid,
            dx: 0.0,
            dy: 0.0,
            flip: false,
            prop: None,
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
        // Every animation starts and ends with the cat at home, facing out.
        (self.dx, self.dy, self.flip, self.prop) = (0.0, 0.0, false, None);
        match anim {
            Anim::Idle => self.idle_len = 60 + (self.rand() * 60.0) as u32,
            Anim::Ball => {
                self.ball = Some(Ball {
                    x: BALL_HOME_X,
                    y: -3.0,
                    vx: 0.0,
                    vy: 0.0,
                });
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
            self.bag = Anim::repertoire(self.env);
            for i in (1..self.bag.len()).rev() {
                let j = (self.rand() * (i + 1) as f32) as usize;
                self.bag.swap(i, j.min(i));
            }
            // The bag is popped from the end; don't open with a repeat.
            if self.bag.last() == Some(&self.last_played) {
                let last = self.bag.len() - 1;
                self.bag.swap(0, last);
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
                } else if let Some(pose) = self.scenery_reaction() {
                    pose
                } else if self.step % 75 >= 71 {
                    Pose::SitBlink
                } else if (self.step / 18) % 4 == 3 {
                    Pose::SitTail
                } else {
                    Pose::Sit
                }
            }
            Anim::Ball => self.step_ball(),
            Anim::Stare => self.step_stare(),
            Anim::Zoomies => self.step_zoomies(),
            Anim::Pounce => self.step_pounce(),
            Anim::Box => self.step_box(),
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
        let asleep = self.anim == Anim::Sleep && matches!(self.pose, Pose::SleepA | Pose::SleepB);
        if asleep && self.step.is_multiple_of(40) {
            self.zs.push((CAT_X as f32 + 5.0, (FLOOR_Y - 10) as f32, 0));
        }
        for z in &mut self.zs {
            z.0 += 0.07;
            z.1 -= 0.14;
            z.2 += 1;
        }
        self.zs.retain(|z| z.2 < 90);
    }

    /// While idle, how the cat responds to something passing through the
    /// scene: it swats at the library's flying book, and bristles when the
    /// night forest's bat or witch appears.
    fn scenery_reaction(&self) -> Option<Pose> {
        match self.env {
            Environment::Library => {
                let (p, slot) = BOOK_EVENT.progress(self.tick)?;
                let (x, y) = book_position(p, slot);
                let overhead = (x + 3.0 - (CAT_X + SPRITE / 2) as f32).abs() < 7.0;
                Some(if overhead && y > 20.0 {
                    Pose::SwatUp
                } else {
                    Pose::LookUp
                })
            }
            Environment::NightForest => {
                let (p, _) = BAT_EVENT
                    .progress(self.tick)
                    .or_else(|| WITCH_EVENT.progress(self.tick))?;
                // A fright at first, then it watches the thing go.
                Some(if p < 0.35 { Pose::Puffed } else { Pose::LookUp })
            }
            _ => None,
        }
    }

    /// A moth crosses overhead; the cat's head follows it.
    fn step_stare(&mut self) -> Pose {
        let t = self.step;
        if t >= STARE_STEPS + 10 {
            self.start(Anim::Idle);
            return Pose::Sit;
        }
        if t >= STARE_STEPS {
            self.prop = None;
            return Pose::SitBlink;
        }
        let p = t as f32 / STARE_STEPS as f32;
        let x = -3.0 + p * (GRID as f32 + 6.0);
        let y = (FLOOR_Y - 27) as f32 + (p * 19.0).sin() * 3.0;
        self.prop = Some(Prop {
            kind: PropKind::Moth,
            x,
            y,
        });
        if x < (CAT_X + SPRITE - 2) as f32 {
            Pose::LookUp
        } else {
            Pose::Tilt
        }
    }

    /// A wind-up, then laps: out of one edge and back in from the other.
    fn step_zoomies(&mut self) -> Pose {
        let t = self.step;
        if t < ZOOM_WIND_UP {
            self.dx = if t % 4 < 2 { 0.0 } else { 1.0 };
            return Pose::Crouch;
        }
        let run = t - ZOOM_WIND_UP;
        let travelled = run as f32 * ZOOM_SPEED;
        let total = ZOOM_LAPS * 2.0 * ZOOM_EDGE;
        if travelled < total {
            // Position on a loop that leaves at +EDGE and re-enters at -EDGE.
            self.dx = (travelled + ZOOM_EDGE).rem_euclid(2.0 * ZOOM_EDGE) - ZOOM_EDGE;
            return run_pose(run);
        }
        self.dx = 0.0;
        if travelled >= total + ZOOM_SKID as f32 * ZOOM_SPEED {
            self.start(Anim::Idle);
            return Pose::Sit;
        }
        Pose::Crouch
    }

    /// What the cat pounces on in this scene, if anything.
    fn prey(&self) -> Option<PropKind> {
        match self.env {
            Environment::Dungeon => Some(PropKind::Spider),
            Environment::Forest => Some(PropKind::Butterfly),
            _ => None,
        }
    }

    /// Which way the cat pounces: +1 right, -1 left. In the dungeon the
    /// cauldron stands to the right, so it goes left there.
    fn pounce_dir(&self) -> f32 {
        if self.env == Environment::Dungeon {
            -1.0
        } else {
            1.0
        }
    }

    fn step_pounce(&mut self) -> Pose {
        let t = self.step;
        let dir = self.pounce_dir();
        let leap_at = POUNCE_STALK + POUNCE_WIGGLE;
        let land_at = leap_at + POUNCE_LEAP;
        let return_at = land_at + POUNCE_LAND;

        // The prey wanders in from the edge during the stalk and bolts once
        // the cat is in the air: the spider along the floor, the butterfly
        // upwards.
        if let Some(kind) = self.prey() {
            let (wait_x, edge_x) = prey_track(dir);
            let rest_y = match kind {
                PropKind::Spider => (FLOOR_Y - 3) as f32,
                _ => (FLOOR_Y - 9) as f32 + (t as f32 * 0.4).sin() * 1.5,
            };
            let (x, y) = if t < POUNCE_STALK {
                let p = t as f32 / POUNCE_STALK as f32;
                (edge_x + (wait_x - edge_x) * p, rest_y)
            } else if t < leap_at + 3 {
                (wait_x, rest_y)
            } else {
                let fled = (t - leap_at - 3) as f32;
                match kind {
                    PropKind::Spider => (wait_x + dir * fled * 1.6, rest_y),
                    _ => (wait_x + dir * fled * 0.9, rest_y - fled * 1.4),
                }
            };
            let gone = (x - edge_x) * dir > 0.0 || y < -4.0;
            self.prop = (!gone).then_some(Prop { kind, x, y });
        }

        // The running sprite faces right, so it is mirrored to go left.
        if t < POUNCE_STALK {
            Pose::Crouch
        } else if t < leap_at {
            // The wiggle before the leap.
            self.dx = if (t / 2).is_multiple_of(2) { 0.0 } else { 1.0 };
            Pose::Crouch
        } else if t < land_at {
            let p = (t - leap_at) as f32 / POUNCE_LEAP as f32;
            self.dx = dir * POUNCE_DIST * p;
            self.dy = -POUNCE_HEIGHT * 4.0 * p * (1.0 - p);
            self.flip = dir < 0.0;
            Pose::RunA
        } else if t < return_at {
            (self.dx, self.dy, self.flip) = (dir * POUNCE_DIST, 0.0, false);
            Pose::Crouch
        } else {
            // Trot back home, facing the other way.
            let back = (t - return_at) as f32;
            if back >= POUNCE_DIST {
                self.start(Anim::Idle);
                return Pose::Sit;
            }
            (self.dx, self.flip) = (dir * (POUNCE_DIST - back), dir > 0.0);
            run_pose(t)
        }
    }

    /// A box slides in; the cat goes over, sits in it for a while, then
    /// comes back and the box slides away.
    fn step_box(&mut self) -> Pose {
        let t = self.step;
        let box_x = (CAT_X + BOX_WALK as i32) as f32;
        let box_y = (FLOOR_Y - BOX_H) as f32;
        let walk_at = BOX_SLIDE + BOX_NOTICE;
        let sit_at = walk_at + BOX_WALK;
        let leave_at = sit_at + BOX_SIT;
        let home_at = leave_at + BOX_WALK;

        let slide = |progress: f32| box_x + (OFFSTAGE_X - box_x) * progress;
        let x = if t < BOX_SLIDE {
            slide(1.0 - t as f32 / BOX_SLIDE as f32)
        } else if t < home_at {
            box_x
        } else {
            slide((t - home_at) as f32 / BOX_SLIDE as f32)
        };
        self.prop = Some(Prop {
            kind: PropKind::Box,
            x,
            y: box_y,
        });

        if t < BOX_SLIDE {
            Pose::Sit
        } else if t < walk_at {
            if t < BOX_SLIDE + BOX_NOTICE / 2 {
                Pose::LookUp
            } else {
                Pose::Tilt
            }
        } else if t < sit_at {
            self.dx = (t - walk_at) as f32;
            run_pose(t)
        } else if t < leave_at {
            self.dx = BOX_WALK as f32;
            if (t - sit_at) % 50 >= 45 {
                Pose::SitBlink
            } else {
                Pose::Sit
            }
        } else if t < home_at {
            (self.dx, self.flip) = ((home_at - t) as f32, true);
            run_pose(t)
        } else if t < home_at + BOX_SLIDE {
            (self.dx, self.flip) = (0.0, false);
            Pose::Sit
        } else {
            self.start(Anim::Idle);
            Pose::Sit
        }
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
            if self.swats > LIGHT_SWATS {
                Pose::Swat
            } else {
                Pose::SwatUp
            }
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
        Self {
            px: vec![fill; (GRID * GRID) as usize],
        }
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
                let (Some(px), true) = (self.get(x, y), d < r) else {
                    continue;
                };
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
                    let col = if mirror {
                        w - 1 - col as i32
                    } else {
                        col as i32
                    };
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

    /// Draw the cat, mirrored if `flip`, part-way (`t` in 0–1) through a
    /// cross-fade from `from` to `to`.  Each cell fades between what the two poses show there (the
    /// background where a pose is transparent), so cells that are the same
    /// in both poses stay solid instead of ghosting.
    #[allow(clippy::too_many_arguments)]
    fn cat(
        &mut self,
        from: &Sprite,
        to: &Sprite,
        coat: Coat,
        x0: i32,
        y0: i32,
        t: f32,
        flip: bool,
    ) {
        for row in 0..SPRITE {
            for col in 0..SPRITE {
                let (x, y) = (x0 + col, y0 + row);
                let Some(bg) = self.get(x, y) else { continue };
                // Mirrored, the sprite's right-hand column is drawn first.
                let source = if flip { SPRITE - 1 - col } else { col };
                let cell = |s: &Sprite| s[row as usize].as_bytes()[source as usize];
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
        c.rect(
            0,
            y,
            GRID - 1,
            y,
            [mix(118.0, 196.0), mix(192.0, 232.0), mix(226.0, 214.0)],
        );
    }
    // Clouds
    c.rect(34, 5, 43, 6, [240, 248, 250]);
    c.rect(36, 4, 41, 4, [240, 248, 250]);
    c.rect(14, 10, 20, 11, [240, 248, 250]);
    // Two rows of pines
    for (i, cx) in (2..GRID).step_by(8).enumerate() {
        c.pine(
            cx,
            18 + (scatter(i as i32, 1) % 6) as i32,
            42,
            [70, 138, 118],
        );
    }
    for (i, cx) in (-2..GRID + 4).step_by(9).enumerate() {
        c.pine(
            cx,
            22 + (scatter(i as i32, 2) % 8) as i32,
            43,
            [40, 100, 80],
        );
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
        let col = if x % 6 < 3 {
            [216, 194, 164]
        } else {
            [206, 182, 150]
        };
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

/// An occasional scenery event (a bat, the witch, the flying book).
///
/// Time is cut into slots of `slot_len` steps; in `chance_pct` percent of them
/// — chosen by a hash of the slot number and `salt`, so it is repeatable — the
/// event plays for the slot's first `duration` steps.
struct Event {
    slot_len: u64,
    duration: u64,
    chance_pct: u32,
    salt: i32,
}

impl Event {
    /// If the event is playing at `tick`: how far through it is (0–1) and the
    /// slot number (handy for varying direction or colour between plays).
    fn progress(&self, tick: u64) -> Option<(f32, u64)> {
        let (slot, within) = (tick / self.slot_len, tick % self.slot_len);
        (within < self.duration && scatter(slot as i32, self.salt) % 100 < self.chance_pct)
            .then(|| (within as f32 / self.duration as f32, slot))
    }
}

/// A bat about every 20 s on average.
const BAT_EVENT: Event = Event {
    slot_len: 300,
    duration: 110,
    chance_pct: 50,
    salt: 5,
};
/// The witch about every 7 minutes on average.
const WITCH_EVENT: Event = Event {
    slot_len: 1500,
    duration: 210,
    chance_pct: 12,
    salt: 3,
};
/// The flying book about every 22 s on average.
const BOOK_EVENT: Event = Event {
    slot_len: 360,
    duration: 130,
    chance_pct: 55,
    salt: 8,
};

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
        c.rect(
            0,
            y,
            GRID - 1,
            y,
            [mix(10.0, 30.0), mix(14.0, 42.0), mix(38.0, 80.0)],
        );
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
    if let Some((p, slot)) = WITCH_EVENT.progress(tick) {
        let leftward = slot % 2 == 1;
        let travel = -WITCH_W - 2.0 + p * (GRID as f32 + 2.0 * WITCH_W + 4.0);
        let x = if leftward {
            GRID as f32 - WITCH_W - travel
        } else {
            travel
        };
        let y = MOON.1 - 5.0 + (p * 9.0).sin() * 0.8;
        c.stamp(
            &WITCH,
            x.round() as i32,
            y.round() as i32,
            [6, 6, 12],
            leftward,
        );
    }
    // Now and then: a bat flapping across the sky
    if let Some((p, slot)) = BAT_EVENT.progress(tick) {
        let x = -10.0 + p * (GRID as f32 + 20.0);
        let x = if slot % 2 == 0 {
            x
        } else {
            GRID as f32 - 9.0 - x
        };
        let y = 13.0 + (scatter(slot as i32, 6) % 8) as f32 + (p * 11.0).sin() * 3.0;
        c.stamp(
            &BAT[(tick / 4 % 2) as usize],
            x.round() as i32,
            y.round() as i32,
            [8, 8, 16],
            false,
        );
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
    c.glow(
        CAT_X as f32 + 8.0,
        FLOOR_Y as f32 + 1.0,
        22.0,
        [60, 90, 100],
        0.6,
    );
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
        c.glow(
            x as f32 + 0.5,
            y as f32 - 1.0,
            6.0,
            cap,
            0.25 + 0.45 * pulse,
        );
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
        let y =
            (scatter(i, 22) % 26) as f32 + 20.0 + (t * (0.017 + f * 0.0011) + f * 2.3).sin() * 3.0;
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
    for &(x, y) in &[
        (3, 1),
        (1, 3),
        (5, 2),
        (2, 5),
        (6, 4),
        (4, 6),
        (5, 3),
        (3, 5),
    ] {
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
    c.glow(
        11.5,
        18.0,
        14.0,
        [255, 150, 60],
        0.45 + 0.1 * (t * 0.4).sin(),
    );

    // A spider letting itself down on a thread, and one pacing the wall
    let legs = (tick / 6 % 2) as usize;
    let drop = (14.0 + (t * 0.02).sin() * 9.0) as i32;
    c.rect(24, 0, 24, drop, [150, 150, 164]);
    c.stamp(&SPIDER[legs], 22, drop, [18, 14, 22], false);
    let crawl = (30.0 + (t * 0.012).sin() * 24.0) as i32;
    c.stamp(
        &SPIDER[legs],
        crawl,
        33 + (scatter(crawl / 6, 40) % 2) as i32,
        [18, 14, 22],
        false,
    );

    // Cauldron over a fire; the brew slowly cycles through the rainbow
    let brew = hue_rgb(t / 480.0);
    let (cx, top) = (50.0, 37);
    for x in 44..=56 {
        let h = scatter(x, (tick / 3) as i32) % 4;
        c.rect(
            x,
            51 - h as i32,
            x,
            52,
            if h > 2 {
                [255, 220, 100]
            } else {
                [255, 130, 40]
            },
        );
    }
    c.rect(43, 52, 57, 53, [70, 46, 32]);
    c.glow(cx, 51.0, 9.0, [255, 140, 50], 0.5);
    c.ellipse(cx, 44.0, 8.0, 6.5, [26, 24, 32]);
    c.ellipse(cx - 2.0, 43.0, 4.0, 3.0, [40, 38, 48]);
    c.rect(41, top, 59, top + 1, [44, 42, 54]);
    c.rect(43, top, 57, top, brew);
    c.rect(
        44,
        top + 1,
        56,
        top + 1,
        brew.map(|v| (v as f32 * 0.7) as u8),
    );
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
        [150, 50, 50],
        [50, 90, 140],
        [60, 120, 80],
        [160, 130, 60],
        [110, 70, 130],
        [150, 90, 50],
        [70, 70, 90],
        [190, 180, 150],
    ];
    // Wall of bookshelves
    c.rect(0, 0, GRID - 1, 41, [40, 28, 22]);
    for shelf in 0..5 {
        let y0 = 1 + shelf * 8;
        let mut x = 1;
        while x < GRID - 1 {
            let w = 1 + (scatter(x, shelf) % 2) as i32;
            if !scatter(x, shelf + 30).is_multiple_of(11) {
                let h = 4 + (scatter(x, shelf + 9) % 3) as i32;
                let col = SPINES[(scatter(x, shelf + 20) % 8) as usize];
                c.rect(x, y0 + 7 - h, x + w - 1, y0 + 6, col);
                if w == 2 {
                    c.set(x, y0 + 8 - h, col.map(|v| v / 2 + 110));
                }
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
        let flicker = 0.78 + 0.12 * (t * 0.31 + f * 2.0).sin() + 0.10 * (t * 0.83 + f * 5.0).sin();
        let gust = scatter((tick / 5) as i32, 50 + i as i32).is_multiple_of(6);
        c.glow(
            x as f32,
            base as f32 - 4.0,
            20.0,
            [255, 170, 80],
            0.75 * flicker,
        );
        c.rect(x, base - 3, x, base, [236, 228, 204]);
        c.set(x, base - 4, [255, 170, 60]);
        c.set(x + gust as i32, base - 5, [255, 236, 150]);
    }

    // Now and then a book flaps past, swooping low through the room
    if let Some((p, slot)) = BOOK_EVENT.progress(tick) {
        let (x, y) = book_position(p, slot);
        let (x, y) = (x.round() as i32, y.round() as i32);
        let cover = SPINES[(slot % 6) as usize];
        let up = (tick / 5).is_multiple_of(2);
        // Covers and pages beat like wings around the spine.
        for dx in 0..7i32 {
            let lift = (dx - 3).abs();
            let wing = if up { 2 - lift.min(2) } else { lift.min(2) };
            c.set(x + dx, y + wing, cover);
            if dx != 3 {
                c.set(x + dx, y + wing + 1, [240, 232, 208]);
            }
        }
        c.set(x + 3, y + if up { 3 } else { 1 }, cover);
    }
}

fn paint_ball(c: &mut Canvas, ball: &Ball) {
    const BALL: [&str; 5] = [".ooo.", "ohrro", "orrro", "orrdo", ".ooo."];
    let (x0, y0) = (
        (ball.x - BALL_RADIUS).round() as i32,
        (ball.y - BALL_RADIUS).round() as i32,
    );
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

const CARDBOARD: [u8; 3] = [176, 134, 84];
const CARDBOARD_DARK: [u8; 3] = [132, 96, 56];

/// The part of a prop that sits behind the cat. Only the box has one: its
/// far rim and the dark inside.
fn paint_prop_behind(c: &mut Canvas, prop: &Prop) {
    if prop.kind == PropKind::Box {
        let (x, y) = (prop.x.round() as i32, prop.y.round() as i32);
        c.rect(x, y - 2, x + BOX_W - 1, y - 2, CARDBOARD_DARK);
        c.rect(x + 1, y - 1, x + BOX_W - 2, y, [70, 50, 32]);
    }
}

/// A prop, or for the box its near side, drawn over the cat.
fn paint_prop(c: &mut Canvas, prop: &Prop, tick: u64) {
    let (x, y) = (prop.x.round() as i32, prop.y.round() as i32);
    let beat = (tick / 4).is_multiple_of(2);
    match prop.kind {
        PropKind::Moth => {
            let wing = if beat { -1 } else { 1 };
            c.set(x, y, [226, 220, 200]);
            c.set(x - 1, y + wing, [190, 184, 166]);
            c.set(x + 1, y + wing, [190, 184, 166]);
        }
        PropKind::Butterfly => {
            const WINGS: [[&str; 3]; 2] = [["o.o", "ooo", "o.o"], [".o.", ".o.", ".o."]];
            c.stamp(&WINGS[usize::from(beat)], x, y, [255, 214, 236], false);
            c.set(x + 1, y + 1, [40, 30, 30]);
        }
        PropKind::Spider => c.stamp(&SPIDER[usize::from(beat)], x, y, [18, 14, 22], false),
        PropKind::Box => {
            c.rect(x, y, x + BOX_W - 1, y + BOX_H - 1, CARDBOARD);
            c.rect(x, y, x + BOX_W - 1, y, CARDBOARD_DARK);
            c.rect(x, y, x, y + BOX_H - 1, CARDBOARD_DARK);
            c.rect(
                x + BOX_W - 1,
                y,
                x + BOX_W - 1,
                y + BOX_H - 1,
                CARDBOARD_DARK,
            );
            // Packing tape down the middle.
            c.rect(
                x + BOX_W / 2 - 1,
                y + 1,
                x + BOX_W / 2,
                y + BOX_H - 1,
                [206, 172, 120],
            );
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
    let cat_cx = CAT_X as f32 + st.dx + 7.0;
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

    if let Some(prop) = &st.prop {
        paint_prop_behind(&mut c, prop);
    }

    // Cat: cross-fade from the previous pose to the current one.
    let t = ((st.since_pose + 1) as f32 / FADE_STEPS as f32).min(1.0);
    c.cat(
        st.prev_pose.sprite(),
        st.pose.sprite(),
        cfg.coat,
        CAT_X + st.dx.round() as i32,
        FLOOR_Y - SPRITE + st.dy.round() as i32,
        t,
        st.flip,
    );

    if let Some(prop) = &st.prop {
        paint_prop(&mut c, prop, st.tick);
    }

    if let Some(ball) = &st.ball {
        paint_ball(&mut c, ball);
    }

    for &(x, y, age) in &st.zs {
        let a = if age < 10 {
            age as f32 / 10.0
        } else {
            ((90 - age) as f32 / 40.0).min(1.0)
        };
        let (x, y) = (x.round() as i32, y.round() as i32);
        for (dx, dy) in [(0, 0), (1, 0), (2, 0), (1, 1), (0, 2), (1, 2), (2, 2)] {
            c.blend(x + dx, y + dy, [250, 250, 255], a * 0.9);
        }
    }
    c
}

/// Advance the simulation to now and render the current frame.
pub fn render_pet_cat() -> DynamicImage {
    let mut guard = STATE
        .get_or_init(|| Mutex::new(PetState::new()))
        .lock()
        .unwrap();
    let st = &mut *guard;

    // Run however many 30 Hz steps have elapsed (capped, so a long pause in
    // rendering doesn't fast-forward through whole animations).
    st.env = config().environment;
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

    const ALL_POSES: [Pose; 26] = [
        Pose::Sit,
        Pose::SitBlink,
        Pose::SitTail,
        Pose::Crouch,
        Pose::Loaf,
        Pose::SleepA,
        Pose::SleepB,
        Pose::LickA,
        Pose::LickB,
        Pose::StretchA,
        Pose::StretchB,
        Pose::LookUp,
        Pose::Swat,
        Pose::SwatUp,
        Pose::SitTail2,
        Pose::SitHalf,
        Pose::SitEar,
        Pose::KneadA,
        Pose::KneadB,
        Pose::Tilt,
        Pose::Yawn,
        Pose::Puffed,
        Pose::GroomA,
        Pose::GroomB,
        Pose::RunA,
        Pose::RunB,
    ];

    #[test]
    fn sprites_are_16x16_with_known_cells() {
        for pose in ALL_POSES {
            for (row, line) in pose.sprite().iter().enumerate() {
                assert_eq!(line.len(), 16, "{pose:?} row {row}");
                assert!(
                    line.bytes().all(|b| b".obwsehcp".contains(&b)),
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
        // Long enough for several rounds of the whole repertoire.
        for _ in 0..30 * 900 {
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
        assert!(seen.contains(&Anim::Idle));
        for anim in Anim::COMMON {
            assert!(seen.contains(&anim), "{anim:?} never played");
        }
        // Scene-specific animations stay out of other scenes.
        assert!(!seen.contains(&Anim::Box) && !seen.contains(&Anim::Watch));
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

    /// First tick at which `event` plays.
    fn first_play(event: &Event) -> u64 {
        (0..)
            .step_by(event.slot_len as usize)
            .find(|&t| event.progress(t).is_some())
            .unwrap()
    }

    fn scene(environment: Environment, tick: u64) -> Canvas {
        let mut st = PetState::new();
        st.tick = tick;
        paint_scene(
            &st,
            PetConfig {
                environment,
                ..PetConfig::DEFAULT
            },
        )
    }

    #[test]
    fn settings_round_trip_and_every_combination_renders() {
        // One test owns the global config, so parallel tests can't race on it.
        for coat in Coat::ALL {
            assert_eq!(Coat::from_str_name(coat.as_str()), Some(coat));
            for environment in Environment::ALL {
                assert_eq!(
                    Environment::from_str_name(environment.as_str()),
                    Some(environment)
                );
                let cfg = PetConfig {
                    coat,
                    environment,
                    background: [1, 2, 3],
                };
                set_config(cfg);
                assert_eq!(config(), cfg);
                let img = render_pet_cat();
                assert_eq!((img.width(), img.height()), (480, 480));
            }
        }
        set_config(PetConfig::DEFAULT);
        assert_eq!(Coat::from_str_name("calico"), None);
        assert_eq!(Environment::from_str_name("Forest"), None);
    }

    #[test]
    fn coats_are_distinct_and_transparent_cells_stay_transparent() {
        for coat in Coat::ALL {
            assert_eq!(coat_color(coat, b'.'), None);
            for cell in *b"obwsecp" {
                assert!(coat_color(coat, cell).is_some());
            }
        }
        let body = |c| coat_color(c, b'b').unwrap();
        assert_ne!(body(Coat::Tuxedo), body(Coat::Orange));
        // The tuxedo has a white chest, the black cat does not…
        assert!(coat_color(Coat::Tuxedo, b'w')
            .unwrap()
            .iter()
            .all(|&v| v > 200));
        assert!(coat_color(Coat::Black, b'w')
            .unwrap()
            .iter()
            .all(|&v| v < 100));
        // …and only the tabby has visible stripes.
        assert_ne!(
            coat_color(Coat::Orange, b's'),
            coat_color(Coat::Orange, b'b')
        );
        assert_eq!(
            coat_color(Coat::Tuxedo, b's'),
            coat_color(Coat::Tuxedo, b'b')
        );
        assert_eq!(coat_color(Coat::Black, b's'), coat_color(Coat::Black, b'b'));
    }

    #[test]
    fn scripted_animations_begin_and_end_by_way_of_the_sitting_cat() {
        for anim in [
            Anim::Sleep,
            Anim::Lick,
            Anim::Stretch,
            Anim::TailFlick,
            Anim::SlowBlink,
            Anim::EarSwivel,
            Anim::Loaf,
            Anim::Knead,
            Anim::Yawn,
            Anim::Groom,
            Anim::Watch,
        ] {
            let mut st = PetState::new();
            st.start(anim);
            let mut poses = Vec::new();
            while st.anim == anim {
                st.step();
                if poses.last() != Some(&st.pose) {
                    poses.push(st.pose);
                }
                assert!(poses.len() < 100, "{anim:?} never finished");
            }
            assert_eq!(st.anim, Anim::Idle);
            assert_eq!(
                st.pose,
                Pose::Sit,
                "{anim:?} should hand back a sitting cat"
            );
            assert!(poses.len() > 2, "{anim:?} shows only {poses:?}");
        }
    }

    #[test]
    fn sleeping_cat_breathes_and_dreams() {
        let mut st = PetState::new();
        st.start(Anim::Sleep);
        let (mut a, mut b, mut most_zs) = (false, false, 0);
        while st.anim == Anim::Sleep {
            st.step();
            a |= st.pose == Pose::SleepA;
            b |= st.pose == Pose::SleepB;
            most_zs = most_zs.max(st.zs.len());
        }
        assert!(a && b, "both breathing frames should show");
        assert!(most_zs >= 1, "no z's floated up");
        // The z's fade out once the cat is awake.
        for _ in 0..100 {
            st.step();
        }
        assert!(st.zs.is_empty());
    }

    #[test]
    fn idle_cat_blinks_and_flicks_its_tail() {
        let mut st = PetState::new();
        st.idle_len = u32::MAX;
        let mut seen = Vec::new();
        for _ in 0..300 {
            st.step();
            if !seen.contains(&st.pose) {
                seen.push(st.pose);
            }
        }
        assert_eq!(st.anim, Anim::Idle);
        for pose in [Pose::Sit, Pose::SitBlink, Pose::SitTail] {
            assert!(seen.contains(&pose), "{pose:?} never shown while idle");
        }
        assert_eq!(seen.len(), 3, "unexpected idle poses: {seen:?}");
    }

    #[test]
    fn ball_falls_from_the_sky_bounces_and_stays_in_play_until_the_last_swat() {
        let mut st = PetState::new();
        st.start(Anim::Ball);
        assert!(
            st.ball.as_ref().unwrap().y < 0.0,
            "ball should start above the screen"
        );
        let floor = FLOOR_Y as f32 - 1.0 - BALL_RADIUS;
        let mut looked_up = false;
        while let Some(ball) = &st.ball {
            assert!(ball.y <= floor + 0.01, "ball sank through the floor");
            if st.swats <= LIGHT_SWATS {
                assert!(
                    (ball.x - BALL_HOME_X).abs() < 8.0,
                    "ball drifted away before the final swat"
                );
            } else {
                assert!(ball.vx > 0.0, "final swat should send the ball right");
            }
            if st.swats == 0 && st.swat_timer == 0 {
                assert!(
                    st.floor_hits <= 2,
                    "cat should bat the ball after two bounces"
                );
            }
            looked_up |= st.pose == Pose::LookUp;
            st.step();
        }
        assert!(looked_up, "cat never watched the ball in the air");
        assert!(st.floor_hits >= 2);
    }

    #[test]
    fn pose_changes_cross_fade_from_the_old_pose_to_the_new() {
        let cfg = PetConfig {
            environment: Environment::Solid,
            ..PetConfig::DEFAULT
        };
        let frame = |pose, prev_pose, since_pose| {
            let mut st = PetState::new();
            (st.pose, st.prev_pose, st.since_pose) = (pose, prev_pose, since_pose);
            paint_scene(&st, cfg).px
        };
        let sit = frame(Pose::Sit, Pose::Sit, FADE_STEPS);
        let loaf = frame(Pose::Loaf, Pose::Loaf, FADE_STEPS);
        assert_ne!(sit, loaf);
        // Finished fade: exactly the new pose, whatever came before.
        assert_eq!(frame(Pose::Loaf, Pose::Sit, FADE_STEPS - 1), loaf);
        // Part-way: neither pose, and no pixel outside the range between them.
        let mid = frame(Pose::Loaf, Pose::Sit, 0);
        assert_ne!(mid, sit);
        assert_ne!(mid, loaf);
        for ((m, a), b) in mid.iter().zip(&sit).zip(&loaf) {
            for i in 0..3 {
                assert!(m[i] >= a[i].min(b[i]) && m[i] <= a[i].max(b[i]));
            }
        }
        // Cells both poses share stay solid rather than ghosting.
        assert_eq!(
            frame(Pose::SitBlink, Pose::Sit, 0)[..GRID as usize * 30],
            sit[..GRID as usize * 30]
        );
    }

    #[test]
    fn canvas_primitives_clip_and_blend() {
        let mut c = Canvas::new([10, 10, 10]);
        c.set(-1, 0, [255; 3]);
        c.set(0, GRID, [255; 3]);
        assert!(
            c.px.iter().all(|&p| p == [10, 10, 10]),
            "out-of-bounds writes must be ignored"
        );
        assert_eq!(c.get(GRID, 0), None);

        c.rect(58, 58, 70, 70, [200, 0, 0]);
        assert_eq!(c.get(59, 59), Some([200, 0, 0]));
        assert_eq!(c.get(57, 57), Some([10, 10, 10]));

        c.blend(0, 0, [110, 210, 10], 0.5);
        assert_eq!(c.get(0, 0), Some([60, 110, 10]));
        c.blend(0, 0, [0, 0, 0], 0.0);
        assert_eq!(c.get(0, 0), Some([60, 110, 10]));

        c.stamp(&["o..", "..."], 10, 10, [1, 2, 3], false);
        c.stamp(&["o..", "..."], 20, 10, [1, 2, 3], true);
        assert_eq!(c.get(10, 10), Some([1, 2, 3]));
        assert_eq!(
            c.get(22, 10),
            Some([1, 2, 3]),
            "mirrored stamp flips left to right"
        );
        assert_eq!(c.get(20, 10), Some([10, 10, 10]));

        let mut lit = Canvas::new([100, 100, 100]);
        lit.glow(30.0, 30.0, 5.0, [100, 0, 0], 1.0);
        assert_eq!(
            lit.get(30, 30),
            Some([200, 100, 100]),
            "full strength at the centre"
        );
        assert_eq!(
            lit.get(40, 30),
            Some([100, 100, 100]),
            "nothing beyond the radius"
        );
        let near = lit.get(32, 30).unwrap()[0];
        assert!(near > 100 && near < 200, "falls off with distance");
        lit.glow(30.0, 30.0, 5.0, [255, 255, 255], 5.0);
        assert_eq!(
            lit.get(30, 30),
            Some([255, 255, 255]),
            "saturates instead of wrapping"
        );

        lit.dim(0.5);
        assert_eq!(lit.get(40, 30), Some([50, 50, 50]));
    }

    #[test]
    fn hue_wheel_hits_the_primaries() {
        assert_eq!(hue_rgb(0.0), [255, 0, 0]);
        assert_eq!(hue_rgb(1.0 / 3.0), [0, 255, 0]);
        assert_eq!(hue_rgb(2.0 / 3.0), [0, 0, 255]);
        assert_eq!(hue_rgb(1.0), hue_rgb(0.0));
        assert_eq!(hue_rgb(-1.0 / 3.0), hue_rgb(2.0 / 3.0));
    }

    #[test]
    fn solid_environment_uses_the_configured_colour() {
        let st = PetState::new();
        let cfg = PetConfig {
            environment: Environment::Solid,
            background: [12, 34, 56],
            ..PetConfig::DEFAULT
        };
        let c = paint_scene(&st, cfg);
        assert_eq!(c.get(0, 0), Some([12, 34, 56]));
        assert_eq!(c.get(GRID - 1, GRID - 1), Some([12, 34, 56]));
        // The cat is drawn on top of it.
        assert_ne!(c.get(CAT_X + 5, FLOOR_Y - 6), Some([12, 34, 56]));
    }

    #[test]
    fn environments_differ_and_only_the_whimsical_ones_animate() {
        let frames: Vec<_> = Environment::ALL.iter().map(|&e| scene(e, 100).px).collect();
        for i in 0..frames.len() {
            for j in i + 1..frames.len() {
                assert_ne!(
                    frames[i],
                    frames[j],
                    "{:?} and {:?} look identical",
                    Environment::ALL[i],
                    Environment::ALL[j]
                );
            }
        }
        for env in [
            Environment::Solid,
            Environment::Forest,
            Environment::LivingRoom,
        ] {
            assert_eq!(
                scene(env, 100).px,
                scene(env, 137).px,
                "{env:?} should be static"
            );
        }
        for env in [
            Environment::NightForest,
            Environment::Dungeon,
            Environment::Library,
        ] {
            assert_ne!(
                scene(env, 100).px,
                scene(env, 137).px,
                "{env:?} should animate"
            );
        }
    }

    #[test]
    fn scenery_events_are_occasional_and_the_witch_is_rare() {
        let hour = 30 * 3600;
        let plays = |e: &Event| {
            (0..hour)
                .step_by(e.slot_len as usize)
                .filter(|&t| e.progress(t).is_some())
                .count()
        };
        for (name, event) in [
            ("bat", &BAT_EVENT),
            ("witch", &WITCH_EVENT),
            ("book", &BOOK_EVENT),
        ] {
            assert!(
                event.duration < event.slot_len,
                "{name} would overlap its next slot"
            );
            assert!(plays(event) > 0, "{name} never plays");
            // Progress runs 0 → just under 1 across the event, then stops.
            let start = first_play(event);
            assert_eq!(event.progress(start).unwrap().0, 0.0);
            assert!(event.progress(start + event.duration - 1).unwrap().0 < 1.0);
            assert!(event.progress(start + event.duration).is_none());
        }
        assert!(plays(&BAT_EVENT) > 100 && plays(&BOOK_EVENT) > 100);
        assert!(
            plays(&WITCH_EVENT) < 20,
            "witch plays {} times an hour",
            plays(&WITCH_EVENT)
        );
    }

    #[test]
    fn witch_passes_in_front_of_the_moon() {
        const SILHOUETTE: [u8; 3] = [6, 6, 12];
        let moon = (MOON.0 as i32, MOON.1 as i32);
        let start = first_play(&WITCH_EVENT);
        assert_ne!(
            scene(Environment::NightForest, start).get(moon.0, moon.1),
            Some(SILHOUETTE)
        );
        let covered = (start..start + WITCH_EVENT.duration).any(|tick| {
            scene(Environment::NightForest, tick).get(moon.0, moon.1) == Some(SILHOUETTE)
        });
        assert!(covered);
    }

    #[test]
    fn bat_and_book_show_up_only_during_their_events() {
        // Colours nothing else in those scenes uses: the bat's silhouette and
        // the flying book's pages.
        let count =
            |env, tick, color: [u8; 3]| scene(env, tick).px.iter().filter(|&&p| p == color).count();
        for (env, event, color) in [
            (Environment::NightForest, &BAT_EVENT, [8, 8, 16]),
            (Environment::Library, &BOOK_EVENT, [240, 232, 208]),
        ] {
            let start = first_play(event);
            assert!(
                count(env, start + event.duration / 2, color) >= 6,
                "{env:?}: nothing flying mid-event"
            );
            assert_eq!(
                count(env, start + event.duration + 5, color),
                0,
                "{env:?}: still there after the event"
            );
        }
    }

    /// Run `anim` in `env` to the end, calling `watch` after every step.
    fn play(env: Environment, anim: Anim, mut watch: impl FnMut(&PetState)) -> PetState {
        let mut st = PetState::new();
        st.env = env;
        st.start(anim);
        let mut steps = 0;
        while st.anim == anim {
            st.step();
            watch(&st);
            steps += 1;
            assert!(steps < 30 * 60, "{anim:?} never finished");
        }
        st
    }

    #[test]
    fn every_scene_has_the_common_animations_plus_its_own() {
        for env in Environment::ALL {
            let anims = Anim::repertoire(env);
            assert!(Anim::COMMON.iter().all(|a| anims.contains(a)), "{env:?}");
            assert!(!anims.contains(&Anim::Idle));
            assert_eq!(anims.contains(&Anim::Box), env == Environment::LivingRoom);
            assert_eq!(
                anims.contains(&Anim::Watch),
                env == Environment::NightForest
            );
        }
        // A full round in the living room does include the box.
        let mut st = PetState::new();
        st.env = Environment::LivingRoom;
        let round: Vec<Anim> = (0..15).map(|_| st.next_from_bag()).collect();
        assert!(round.contains(&Anim::Box), "{round:?}");
    }

    #[test]
    fn animations_that_move_the_cat_bring_it_home_facing_out() {
        for env in Environment::ALL {
            for anim in [Anim::Stare, Anim::Zoomies, Anim::Pounce, Anim::Box] {
                let st = play(env, anim, |_| {});
                assert_eq!(st.anim, Anim::Idle, "{env:?} {anim:?}");
                assert_eq!(
                    (st.dx, st.dy, st.flip),
                    (0.0, 0.0, false),
                    "{env:?} {anim:?}"
                );
                assert_eq!(st.prop, None, "{env:?} {anim:?}");
                assert_eq!(st.pose, Pose::Sit, "{env:?} {anim:?}");
            }
        }
    }

    #[test]
    fn zoomies_leave_by_one_edge_and_return_by_the_other() {
        let (mut left, mut right, mut wraps, mut last) = (0.0f32, 0.0f32, 0, 0.0f32);
        let mut ran = false;
        play(Environment::Solid, Anim::Zoomies, |st| {
            left = left.min(st.dx);
            right = right.max(st.dx);
            // A jump from far right to far left is the cat going round.
            if last - st.dx > ZOOM_EDGE {
                wraps += 1;
            }
            last = st.dx;
            ran |= matches!(st.pose, Pose::RunA | Pose::RunB);
        });
        // Off-screen means the whole sprite is past the edge.
        assert!(CAT_X as f32 + right >= GRID as f32, "never left: {right}");
        assert!(
            CAT_X as f32 + left + SPRITE as f32 <= 0.0,
            "never came in from the left: {left}"
        );
        assert_eq!(wraps, ZOOM_LAPS as i32);
        assert!(ran);
    }

    #[test]
    fn pounce_leaps_at_the_prey_and_the_prey_gets_away() {
        for (env, prey) in [
            (Environment::Dungeon, Some(PropKind::Spider)),
            (Environment::Forest, Some(PropKind::Butterfly)),
            (Environment::Solid, None),
        ] {
            let (mut highest, mut furthest, mut faced_left) = (0.0f32, 0.0f32, false);
            let dir = if env == Environment::Dungeon {
                -1.0
            } else {
                1.0
            };
            let mut kinds = Vec::new();
            let mut waited_at = None;
            play(env, Anim::Pounce, |st| {
                highest = highest.min(st.dy);
                furthest = furthest.max(st.dx * dir);
                faced_left |= st.flip;
                if let Some(prop) = st.prop {
                    if !kinds.contains(&prop.kind) {
                        kinds.push(prop.kind);
                    }
                    if st.step == POUNCE_STALK + 2 {
                        waited_at = Some(prop.x);
                    }
                }
            });
            assert!(
                highest <= -POUNCE_HEIGHT + 0.5,
                "{env:?}: barely left the ground"
            );
            assert_eq!(furthest, POUNCE_DIST, "{env:?}");
            assert!(
                faced_left,
                "{env:?}: should face left on the way out or back"
            );
            assert_eq!(kinds, prey.into_iter().collect::<Vec<_>>(), "{env:?}");
            // The prey sits where the leap ends, in front of the cat.
            assert_eq!(waited_at, prey.map(|_| prey_track(dir).0), "{env:?}");
        }
    }

    #[test]
    fn dungeon_pounce_goes_left_clear_of_the_cauldron() {
        let mut reach = 0.0f32;
        let mut prey_x = Vec::new();
        play(Environment::Dungeon, Anim::Pounce, |st| {
            reach = reach.min(st.dx);
            prey_x.extend(st.prop.map(|p| p.x));
        });
        assert_eq!(reach, -POUNCE_DIST);
        // The spider stays on the floor to the cat's left and on screen
        // while it waits.
        let wait = prey_track(-1.0).0;
        assert!(wait >= 0.0 && wait + PREY_W <= CAT_X as f32, "{wait}");
        assert!(prey_x.iter().all(|x| *x <= wait), "{prey_x:?}");
        // Elsewhere the prey is to the right of where the cat sits.
        assert!(prey_track(1.0).0 >= (CAT_X + SPRITE) as f32);
    }

    #[test]
    fn cat_sits_in_the_box_with_only_its_top_half_showing() {
        let cfg = PetConfig {
            environment: Environment::LivingRoom,
            ..PetConfig::DEFAULT
        };
        let mut seated = None;
        play(Environment::LivingRoom, Anim::Box, |st| {
            let sit_at = BOX_SLIDE + BOX_NOTICE + BOX_WALK;
            if st.step == sit_at + 30 {
                seated = Some((st.dx, st.prop, paint_scene(st, cfg)));
            }
        });
        let (dx, prop, canvas) = seated.expect("the cat never sat");
        let prop = prop.expect("the box left early");
        assert_eq!(prop.kind, PropKind::Box);
        // The cat is in the box, not beside it.
        assert_eq!(CAT_X as f32 + dx, prop.x);
        // Its feet are behind cardboard; its head is above the rim.
        let (x, feet_y, head_y) = (prop.x as i32 + 4, FLOOR_Y - 2, FLOOR_Y - SPRITE + 5);
        assert_eq!(canvas.get(x, feet_y), Some(CARDBOARD));
        let eye = coat_color(cfg.coat, b'e').unwrap();
        assert_eq!(canvas.get(prop.x as i32 + 3, head_y), Some(eye));
    }

    #[test]
    fn stare_follows_a_moth_from_one_side_to_the_other() {
        let (mut first, mut last) = (None, None);
        let mut poses = Vec::new();
        play(Environment::Solid, Anim::Stare, |st| {
            if let Some(prop) = st.prop {
                assert_eq!(prop.kind, PropKind::Moth);
                first.get_or_insert(prop.x);
                last = Some(prop.x);
            }
            if poses.last() != Some(&st.pose) {
                poses.push(st.pose);
            }
        });
        assert!(first.unwrap() < 0.0 && last.unwrap() > (GRID - 2) as f32);
        // Looks up as it approaches, tilts its head as it passes.
        assert_eq!(poses[..2], [Pose::LookUp, Pose::Tilt], "{poses:?}");
    }

    /// The idle poses seen in `env` over `steps` steps starting at `tick`.
    fn idle_poses(env: Environment, tick: u64, steps: u64) -> Vec<Pose> {
        let mut st = PetState::new();
        (st.env, st.tick, st.idle_len) = (env, tick, u32::MAX);
        let mut seen = Vec::new();
        for _ in 0..steps {
            st.step();
            if !seen.contains(&st.pose) {
                seen.push(st.pose);
            }
        }
        seen
    }

    #[test]
    fn library_cat_swats_at_the_flying_book() {
        let flight = first_play(&BOOK_EVENT);
        let during = idle_poses(Environment::Library, flight, BOOK_EVENT.duration);
        // It watches the book come in, swats as it passes overhead, then
        // watches it go.
        assert_eq!(during, [Pose::LookUp, Pose::SwatUp], "{during:?}");
        // Other scenes ignore the same moment; so does the library once
        // the book has gone.
        let elsewhere = idle_poses(Environment::Dungeon, flight, BOOK_EVENT.duration);
        assert!(!elsewhere.contains(&Pose::SwatUp) && !elsewhere.contains(&Pose::LookUp));
        let after = idle_poses(Environment::Library, flight + BOOK_EVENT.duration, 60);
        assert!(!after.contains(&Pose::SwatUp) && !after.contains(&Pose::LookUp));
    }

    #[test]
    fn night_forest_cat_bristles_at_the_bat_and_the_witch() {
        for event in [&BAT_EVENT, &WITCH_EVENT] {
            let start = first_play(event);
            let during = idle_poses(Environment::NightForest, start, event.duration);
            assert_eq!(during, [Pose::Puffed, Pose::LookUp], "{during:?}");
            let elsewhere = idle_poses(Environment::Forest, start, event.duration);
            assert!(!elsewhere.contains(&Pose::Puffed), "{elsewhere:?}");
        }
    }

    #[test]
    fn dozing_in_a_loaf_makes_no_zs() {
        let mut most = 0;
        play(Environment::Solid, Anim::Loaf, |st| {
            most = most.max(st.zs.len());
        });
        assert_eq!(most, 0);
    }

    #[test]
    fn a_flipped_cat_is_the_mirror_image() {
        let draw = |flip| {
            let mut c = Canvas::new([1, 2, 3]);
            c.cat(&RUN_A, &RUN_A, Coat::Orange, 10, 10, 1.0, flip);
            c
        };
        let (normal, flipped) = (draw(false), draw(true));
        assert_ne!(normal.px, flipped.px);
        for row in 0..SPRITE {
            for col in 0..SPRITE {
                assert_eq!(
                    flipped.get(10 + col, 10 + row),
                    normal.get(10 + SPRITE - 1 - col, 10 + row)
                );
            }
        }
    }

    #[test]
    fn half_closed_eyes_are_between_open_and_closed() {
        for coat in Coat::ALL {
            let [open, half, closed] = [b'e', b'h', b'c'].map(|ch| coat_color(coat, ch).unwrap());
            assert!(half != open && half != closed, "{coat:?}");
            for i in 0..3 {
                assert!(half[i] >= open[i].min(closed[i]) && half[i] <= open[i].max(closed[i]));
            }
        }
    }
}
