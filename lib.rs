use std::sync::{Arc, Mutex};
use std::ffi::c_void;
use ruffle_core::backend::{
    audio::NullAudioBackend,
    navigator::NullNavigatorBackend,
    storage::MemoryStorageBackend,
    ui::NullUiBackend,
    video::NullVideoBackend,
    render::NullRenderer,
    log::LogBackend,
};
use ruffle_core::Player;

// --- 1. The Embedded "Hello World" SWF ---
// This acts as a "cartridge" hardcoded in your app for testing.
// It simply prints "Hello 3DS" to the log.
static HELLO_SWF: &[u8] = &[
    0x46, 0x57, 0x53, 0x08, 0x35, 0x00, 0x00, 0x00, 0x78, 0x00, 0x05, 0x5F,
    0x00, 0x00, 0x0F, 0xA0, 0x00, 0x00, 0x0C, 0x01, 0x00, 0x43, 0x02, 0xFF,
    0xFF, 0xFF, 0x3F, 0x03, 0x14, 0x00, 0x00, 0x00, 0x96, 0x0D, 0x00, 0x48,
    0x65, 0x6C, 0x6C, 0x6F, 0x20, 0x33, 0x44, 0x53, 0x00, 0x96, 0x02, 0x00,
    0x26, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00,
];

// --- 2. Custom Logger for 3DS ---
// This intercepts Ruffle logs and prints them to the 3DS stdout (your console).
struct ThreeDSLogger;
impl LogBackend for ThreeDSLogger {
    fn avm_trace(&self, message: &str) {
        // This handles "trace(...)" calls from ActionScript
        println!("[AVM] {}", message);
    }
    fn log(&self, text: &str, level: log::Level) {
        // This handles internal Ruffle engine logs
        println!("[Ruffle:{}] {}", level, text);
    }
}

pub struct BridgeContext {
    player: Arc<Mutex<Player>>,
}

#[no_mangle]
pub extern "C" fn bridge_player_create() -> *mut BridgeContext {
    // A. Setup Backends
    let renderer = Box::new(NullRenderer::new());
    let audio = Box::new(NullAudioBackend::new());
    let navigator = Box::new(NullNavigatorBackend::new());
    let storage = Box::new(MemoryStorageBackend::default());
    let video = Box::new(NullVideoBackend::new());
    let ui = Box::new(NullUiBackend::new());
    
    // Use our custom logger!
    let log = Box::new(ThreeDSLogger);

    // B. Build Player
    let player = Player::builder()
        .with_renderer(renderer)
        .with_audio(audio)
        .with_navigator(navigator)
        .with_storage(storage)
        .with_video(video)
        .with_log(log)
        .with_ui(ui)
        .build();

    let player_arc = Arc::new(Mutex::new(player));

    // C. Load our hardcoded Hello World SWF
    {
        let mut p = player_arc.lock().unwrap();
        let movie = ruffle_core::tag_utils::SwfMovie::from_data(
            HELLO_SWF.to_vec(), 
            None, 
            None
        ).expect("Failed to load embedded SWF");
        
        p.fetch_root_movie(movie);
    }

    Box::into_raw(Box::new(BridgeContext { player: player_arc }))
}

#[no_mangle]
pub extern "C" fn bridge_tick(ctx: *mut BridgeContext) {
    if ctx.is_null() { return; }
    let ctx = unsafe { &*ctx };
    let mut player = ctx.player.lock().unwrap();
    // Run the engine for 1 frame (approx 33ms)
    player.tick(33.33); 
}