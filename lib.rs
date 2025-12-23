use std::sync::{Arc, Mutex, Once};
use std::sync::atomic::{AtomicU32, Ordering};
use std::pin::Pin;
use std::future::Future;
use std::borrow::Cow;
use std::task::{Context, Waker, RawWaker, RawWakerVTable};

// --- Imports ---
use ruffle_core::{Player, PlayerBuilder};
use ruffle_core::swf::{self, SoundInfo, SoundStreamHead, SoundFormat};
use ruffle_core::socket::{SocketHandle, SocketAction};
use ruffle_core::Color;

// Render
use ruffle_render::backend::{
    RenderBackend, ViewportDimensions, Context3D, Context3DProfile,
    ShapeHandle, PixelBenderOutput, PixelBenderTarget
};
use ruffle_render::bitmap::{Bitmap, BitmapHandle, BitmapInfo, SyncHandle, BitmapSource, RgbaBufRead};
use ruffle_render::commands::CommandList;
use ruffle_render::error::Error as RenderError;
use ruffle_render::quality::StageQuality;
use ruffle_render::shape_utils::DistilledShape;
use ruffle_render::pixel_bender::{PixelBenderShader, PixelBenderShaderHandle};
use ruffle_render::pixel_bender_support::PixelBenderShaderArgument;

// Audio
use ruffle_core::backend::audio::{
    AudioBackend, SoundHandle, SoundInstanceHandle, SoundTransform, 
    RegisterError, SoundStreamInfo, Substream, DecodeError
};

// Navigator
use ruffle_core::backend::navigator::{
    NavigatorBackend, NavigationMethod, Request, SuccessResponse, ErrorResponse
};
use ruffle_core::backend::ui::DialogLoaderError;

// UI, Storage, Log
use ruffle_core::backend::storage::StorageBackend;
use ruffle_core::backend::ui::{
    UiBackend, MouseCursor, FileFilter, FileDialogResult, 
    FontDefinition, LanguageIdentifier
};
use ruffle_core::font::FontQuery; 
use ruffle_core::backend::log::LogBackend;

// Video
use ruffle_video::backend::VideoBackend;
use ruffle_video::VideoStreamHandle; 
use ruffle_video::error::Error as VideoError;

// External
use url::Url;
use indexmap::IndexMap;
use async_channel::{Sender, Receiver};

// --- Embedded "Hello World" SWF (Flash 6) ---
static HELLO_SWF: &[u8] = &[
    0x46, 0x57, 0x53, 0x06, 0x30, 0x00, 0x00, 0x00, 0x78, 0x00, 0x05, 0x5F,
    0x00, 0x00, 0x0F, 0xA0, 0x00, 0x00, 0x0C, 0x01, 0x00, 0x43, 0x02, 0xFF,
    0xFF, 0xFF, 0x10, 0x03, 0x96, 0x0B, 0x00, 0x00, 0x48, 0x65, 0x6C, 0x6C,
    0x6F, 0x20, 0x33, 0x44, 0x53, 0x00, 0x26, 0x00, 0x40, 0x00, 0x00, 0x00,
];

// --- 1. System Logger ---
struct ConsoleLogger;
impl log::Log for ConsoleLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool { true }
    fn log(&self, record: &log::Record) {
        // Filter out noisy crates, keep AVM logs
        if record.level() <= log::Level::Info || record.target().contains("avm") {
            println!("[{}] {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}
static LOGGER: ConsoleLogger = ConsoleLogger;
static INIT_LOGGER: Once = Once::new();

// --- 2. Async Executor ---
type BoxedFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;

unsafe fn dummy_waker_clone(_: *const ()) -> RawWaker { dummy_waker() }
unsafe fn dummy_waker_wake(_: *const ()) {}
unsafe fn dummy_waker_wake_by_ref(_: *const ()) {}
unsafe fn dummy_waker_drop(_: *const ()) {}

const VTABLE: RawWakerVTable = RawWakerVTable::new(
    dummy_waker_clone,
    dummy_waker_wake,
    dummy_waker_wake_by_ref,
    dummy_waker_drop,
);

fn dummy_waker() -> RawWaker {
    RawWaker::new(std::ptr::null(), &VTABLE)
}

// --- 3. Memory Response ---
struct MemoryResponse {
    url: String,
    data: Vec<u8>,
}

impl SuccessResponse for MemoryResponse {
    fn url(&self) -> Cow<'_, str> { Cow::Borrowed(&self.url) }
    fn status(&self) -> u16 { 200 }
    fn redirected(&self) -> bool { false }

    fn expected_length(&self) -> Result<Option<u64>, DialogLoaderError> {
        Ok(Some(self.data.len() as u64))
    }

    fn body(self: Box<Self>) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, DialogLoaderError>>>> {
        println!("[3DS] body() called for {}", self.url);
        let data = self.data.clone();
        Box::pin(async move { 
            println!("[3DS] Returning {} bytes", data.len());
            Ok(data) 
        })
    }

    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Result<Option<Vec<u8>>, DialogLoaderError>>>> {
        Box::pin(async move { Ok(None) })
    }

    fn text_encoding(&self) -> Option<&'static encoding_rs::Encoding> {
        None
    }
    
    fn set_url(&mut self, url: String) {
        self.url = url;
    }
}

// --- 4. Backend ---
#[derive(Clone)]
struct ThreeDSBackend {
    tasks: Arc<Mutex<Vec<BoxedFuture>>>,
}

impl ThreeDSBackend {
    fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

// -- Render Backend --
impl RenderBackend for ThreeDSBackend {
    fn viewport_dimensions(&self) -> ViewportDimensions {
        ViewportDimensions { width: 400, height: 240, scale_factor: 1.0 }
    }
    fn set_viewport_dimensions(&mut self, _dimensions: ViewportDimensions) {}

    fn register_shape(&mut self, _shape: DistilledShape<'_>, _bitmap: &dyn BitmapSource) -> ShapeHandle {
        println!("[3DS] register_shape called!");
        unimplemented!("Shapes not supported")
    }
    
    fn submit_frame(&mut self, _clear: Color, _commands: CommandList, _cache: Vec<ruffle_render::backend::BitmapCacheEntry>) {}

    fn render_offscreen(&mut self, _handle: BitmapHandle, _commands: CommandList, _quality: StageQuality, _region: ruffle_render::bitmap::PixelRegion) -> Option<Box<dyn SyncHandle>> { None }

    fn create_empty_texture(&mut self, _width: u32, _height: u32) -> Result<BitmapHandle, RenderError> {
        Err(RenderError::Unimplemented("stub".into()))
    }
    fn register_bitmap(&mut self, _bitmap: Bitmap) -> Result<BitmapHandle, RenderError> {
        Err(RenderError::Unimplemented("stub".into()))
    }
    fn update_texture(&mut self, _handle: &BitmapHandle, _bitmap: Bitmap, _region: ruffle_render::bitmap::PixelRegion) -> Result<(), RenderError> { Ok(()) }
    fn create_context3d(&mut self, _profile: Context3DProfile) -> Result<Box<dyn Context3D>, RenderError> {
        Err(RenderError::Unimplemented("stub".into()))
    }
    fn debug_info(&self) -> Cow<'static, str> { Cow::Borrowed("3DS") }
    fn name(&self) -> &'static str { "3DS" }
    fn set_quality(&mut self, _quality: StageQuality) {}

    fn compile_pixelbender_shader(&mut self, _shader: PixelBenderShader) -> Result<PixelBenderShaderHandle, RenderError> {
        Err(RenderError::Unimplemented("stub".into()))
    }
    fn run_pixelbender_shader(&mut self, _handle: PixelBenderShaderHandle, _args: &[PixelBenderShaderArgument], _target: &PixelBenderTarget) -> Result<PixelBenderOutput, RenderError> {
        Err(RenderError::Unimplemented("stub".into()))
    }
    
    fn resolve_sync_handle(&mut self, _handle: Box<dyn SyncHandle>, _callback: RgbaBufRead) -> Result<(), RenderError> { Ok(()) }
}

// -- Audio Backend --
impl AudioBackend for ThreeDSBackend {
    fn play(&mut self) {}
    fn pause(&mut self) {}
    fn set_volume(&mut self, _volume: f32) {}
    fn register_sound(&mut self, _sound: &swf::Sound) -> Result<SoundHandle, RegisterError> { unimplemented!() }
    fn register_mp3(&mut self, _data: &[u8]) -> Result<SoundHandle, DecodeError> { unimplemented!() }
    fn start_sound(&mut self, _sound: SoundHandle, _settings: &SoundInfo) -> Result<SoundInstanceHandle, DecodeError> { unimplemented!() }
    fn start_stream(&mut self, _stream: ruffle_core::tag_utils::SwfSlice, _stream_info: &SoundStreamHead) -> Result<SoundInstanceHandle, DecodeError> { unimplemented!() }
    fn start_substream(&mut self, _substream: Substream, _info: &SoundStreamInfo) -> Result<SoundInstanceHandle, DecodeError> { unimplemented!() }
    fn stop_sound(&mut self, _sound: SoundInstanceHandle) {}
    fn stop_all_sounds(&mut self) {}
    fn get_sound_position(&self, _sound: SoundInstanceHandle) -> Option<f64> { None }
    fn get_sound_duration(&self, _sound: SoundHandle) -> Option<f64> { None }
    fn get_sound_size(&self, _sound: SoundHandle) -> Option<u32> { None }
    fn get_sound_format(&self, _sound: SoundHandle) -> Option<&SoundFormat> { None }
    fn set_sound_transform(&mut self, _sound: SoundInstanceHandle, _transform: SoundTransform) {}
    fn get_sound_peak(&mut self, _sound: SoundInstanceHandle) -> Option<[f32; 2]> { None }
    fn volume(&self) -> f32 { 1.0 }
    fn get_sample_history(&self) -> [[f32; 2]; 1024] { [[0.0; 2]; 1024] }
}

// -- Navigator Backend --
impl NavigatorBackend for ThreeDSBackend {
    fn navigate_to_url(&self, _url: &str, _target: &str, _vars: Option<(NavigationMethod, IndexMap<String, String>)>) {}
    
    fn fetch(&self, request: Request) -> Pin<Box<dyn Future<Output = Result<Box<dyn SuccessResponse>, ErrorResponse>>>> {
        let url = request.url().to_string();
        println!("[3DS] Fetching URL: {}", url);

        if url.contains("test.swf") {
            let response = MemoryResponse {
                url: url,
                data: HELLO_SWF.to_vec(),
            };
            Box::pin(async move { Ok(Box::new(response) as Box<dyn SuccessResponse>) })
        } else {
             Box::pin(async move { 
                 Err(ErrorResponse {
                     url: url,
                     error: std::io::Error::new(std::io::ErrorKind::NotFound, "Not found").into()
                 }) 
             })
        }
    }

    fn resolve_url(&self, url: &str) -> Result<Url, url::ParseError> { Url::parse(url) }

    fn spawn_future(&mut self, future: Pin<Box<dyn Future<Output = Result<(), DialogLoaderError>>>>) {
        // println!("[3DS] Spawning background task...");
        let mut tasks = self.tasks.lock().unwrap();
        tasks.push(Box::pin(async move { 
            match future.await {
                Ok(_) => println!("[3DS] Task finished: Ok"),
                Err(e) => println!("[3DS] Task finished: Error {:?}", e),
            }
        }));
    }

    fn pre_process_url(&self, url: Url) -> Url { url }
    fn connect_socket(&mut self, _host: String, _port: u16, _timeout: std::time::Duration, _handle: SocketHandle, _receiver: Receiver<Vec<u8>>, _sender: Sender<SocketAction>) {}
}

// -- Storage Backend --
impl StorageBackend for ThreeDSBackend {
    fn get(&self, _key: &str) -> Option<Vec<u8>> { None }
    fn put(&mut self, _key: &str, _value: &[u8]) -> bool { false }
    fn remove_key(&mut self, _key: &str) {}
}

// -- UI Backend --
impl UiBackend for ThreeDSBackend {
    fn mouse_visible(&self) -> bool { true }
    fn set_mouse_visible(&mut self, _visible: bool) {}
    fn set_mouse_cursor(&mut self, _cursor: MouseCursor) {}
    fn clipboard_content(&mut self) -> String { String::new() }
    fn set_clipboard_content(&mut self, _content: String) {}
    fn set_fullscreen(&mut self, _is_full: bool) -> Result<(), Cow<'static, str>> { Ok(()) }
    fn display_root_movie_download_failed_message(&self, _unknown: bool, _msg: String) {}
    fn message(&self, _message: &str) {}
    fn open_virtual_keyboard(&self) {}
    fn close_virtual_keyboard(&self) {}
    fn language(&self) -> LanguageIdentifier { "en-US".parse().unwrap() }
    fn display_unsupported_video(&self, _url: Url) {}
    fn load_device_font(&self, _query: &FontQuery, _callback: &mut dyn FnMut(FontDefinition)) {}
    fn sort_device_fonts(&self, _query: &FontQuery, _callback: &mut dyn FnMut(FontDefinition)) -> Vec<FontQuery> { vec![] }
    fn display_file_open_dialog(&mut self, _filter: Vec<FileFilter>) -> Option<Pin<Box<dyn Future<Output = Result<Box<dyn FileDialogResult>, DialogLoaderError>>>>> { None }
    fn display_file_save_dialog(&mut self, _title: String, _filter: String) -> Option<Pin<Box<dyn Future<Output = Result<Box<dyn FileDialogResult>, DialogLoaderError>>>>> { None }
    fn close_file_dialog(&mut self) {}
}

// -- Log Backend --
impl LogBackend for ThreeDSBackend {
    fn avm_trace(&self, message: &str) { println!("[AVM] {}", message); }
    fn avm_warning(&self, message: &str) { println!("[AVM Warn] {}", message); }
}

// -- Video Backend --
impl VideoBackend for ThreeDSBackend {
    fn register_video_stream(&mut self, _num_frames: u32, _size: (u16, u16), _codec: swf::VideoCodec, _deblocking: swf::VideoDeblocking) -> Result<VideoStreamHandle, VideoError> { unimplemented!() }
    fn configure_video_stream_decoder(&mut self, _handle: VideoStreamHandle, _header: &[u8]) -> Result<(), VideoError> { unimplemented!() }
    fn preload_video_stream_frame(&mut self, _handle: VideoStreamHandle, _frame: ruffle_video::frame::EncodedFrame<'_>) -> Result<ruffle_video::frame::FrameDependency, VideoError> { unimplemented!() }
    fn decode_video_stream_frame(&mut self, _handle: VideoStreamHandle, _frame: ruffle_video::frame::EncodedFrame<'_>, _renderer: &mut dyn RenderBackend) -> Result<BitmapInfo, VideoError> { unimplemented!() }
}

pub struct BridgeContext {
    player: Arc<Mutex<Player>>,
    backend: ThreeDSBackend,
}

#[no_mangle]
pub extern "C" fn bridge_player_create() -> *mut BridgeContext {
    INIT_LOGGER.call_once(|| {
        log::set_logger(&LOGGER).unwrap();
        log::set_max_level(log::LevelFilter::Trace);
    });

    let backend = ThreeDSBackend::new();

    let player = PlayerBuilder::new()
        .with_renderer(backend.clone())
        .with_audio(backend.clone())
        .with_navigator(backend.clone())
        .with_storage(Box::new(backend.clone()))
        .with_video(backend.clone())
        .with_log(backend.clone())
        .with_ui(backend.clone())
        .build();

    let player_arc = player;

    println!("[3DS] Requesting Movie Load...");
    {
        let mut p = player_arc.lock().unwrap();
        p.fetch_root_movie("test.swf".to_string(), vec![], Box::new(|_| {
            println!("[3DS] Root movie fetched callback!");
        }));
    }

    Box::into_raw(Box::new(BridgeContext { 
        player: player_arc,
        backend: backend,
    }))
}

// Use AtomicU32 for safe global counter
static TICK_COUNTER: AtomicU32 = AtomicU32::new(0);

#[no_mangle]
pub extern "C" fn bridge_tick(ctx: *mut BridgeContext) {
    if ctx.is_null() { return; }
    let ctx = unsafe { &*ctx };
    
    // 1. Run Async Tasks
    {
        let mut tasks = ctx.backend.tasks.lock().unwrap();
        let waker = unsafe { Waker::from_raw(dummy_waker()) };
        let mut context = Context::from_waker(&waker);

        tasks.retain_mut(|task| {
            task.as_mut().poll(&mut context).is_pending()
        });
    }

    // 2. Tick Ruffle
    let mut player = ctx.player.lock().unwrap();
    player.tick(33.33); 

    // 3. Heartbeat
    let ticks = TICK_COUNTER.fetch_add(1, Ordering::Relaxed);
    if ticks % 60 == 0 {
        println!("[3DS] Tick {}", ticks);
    }
}