use std::sync::{Arc, Mutex};
use std::ffi::c_void;
use ruffle_core::backend::{
    audio::NullAudioBackend, 
    navigator::NullNavigatorBackend, 
    storage::MemoryStorageBackend,
    ui::NullUiBackend,
    log::NullLogBackend,
    video::NullVideoBackend,
    render::NullRenderer, // We will replace this later!
};
use ruffle_core::Player;

// Wrapper to hold the player instance across C calls
pub struct BridgeContext {
    player: Arc<Mutex<Player>>,
}

#[no_mangle]
pub extern "C" fn bridge_player_create(
    swf_ptr: *const u8,
    swf_len: usize,
) -> *mut BridgeContext {
    let swf_bytes = unsafe { std::slice::from_raw_parts(swf_ptr, swf_len) };
    
    // 1. Setup Backends
    // For now, we use "Null" backends which do nothing. 
    // In Phase 2, we will implement a custom RenderBackend.
    let renderer = Box::new(NullRenderer::new());
    let audio = Box::new(NullAudioBackend::new());
    let navigator = Box::new(NullNavigatorBackend::new());
    let storage = Box::new(MemoryStorageBackend::default());
    let video = Box::new(NullVideoBackend::new());
    let log = Box::new(NullLogBackend::new());
    let ui = Box::new(NullUiBackend::new());

    // 2. Create Player
    // The builder might vary slightly depending on Ruffle version
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

    // 3. Load Data
    // We clone the Arc to pass into the loader, but here we just call load_data
    {
        let mut p = player_arc.lock().unwrap();
        // movie_url is optional, used for relative paths
        p.fetch_root_movie(
            ruffle_core::tag_utils::SwfMovie::from_data(swf_bytes.to_vec(), None, None).expect("Bad SWF")
        );
    }

    Box::into_raw(Box::new(BridgeContext { player: player_arc }))
}

#[no_mangle]
pub extern "C" fn bridge_tick(ctx: *mut BridgeContext, dt_ms: f64) {
    if ctx.is_null() { return; }
    let ctx = unsafe { &*ctx };
    
    let mut player = ctx.player.lock().unwrap();
    // This runs the AVM logic!
    player.tick(dt_ms); 
}

// ... Add destroy function ...