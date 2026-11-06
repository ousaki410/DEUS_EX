//! deusex-audio - OpenAL Soft 音频后端（声音合成器与音频通道）.
//!
//! 当系统安装了 OpenAL 开发库后，启用 `openal` feature 即可编译真实后端：
//!   cargo build -p deusex-audio --features openal
//! 未启用时保持纯 Rust placeholder，不依赖任何系统库.
//!
//! 本模块提供：
//! - `AudioContext`: OpenAL 设备与上下文的 RAII 管理
//! - `AudioBuffer`: 加载 PCM/WAV 音频数据到 OpenAL 缓冲
//! - `AudioSource`: 音源，控制播放、音量、音调、循环、3D 位置
//! - `AudioManager`: 统一管理缓冲与音源，提供高层播放接口

use std::collections::HashMap;

use tracing::{debug, info};

/// 3D 空间位置。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Default for Position {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, z: 0.0 }
    }
}

/// 音频监听者（玩家/摄像机）状态。
#[derive(Debug, Clone, Copy)]
pub struct Listener {
    pub position: Position,
    pub forward: Position,
    pub up: Position,
    pub velocity: Position,
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            position: Position::default(),
            forward: Position { x: 0.0, y: 0.0, z: -1.0 },
            up: Position { x: 0.0, y: 1.0, z: 0.0 },
            velocity: Position::default(),
        }
    }
}

/// 音源状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceState {
    Initial,
    Playing,
    Paused,
    Stopped,
}

/// 音源配置。
#[derive(Debug, Clone, Copy)]
pub struct SourceConfig {
    pub gain: f32,
    pub pitch: f32,
    pub looping: bool,
    pub position: Position,
    pub velocity: Position,
}

impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            gain: 1.0,
            pitch: 1.0,
            looping: false,
            position: Position::default(),
            velocity: Position::default(),
        }
    }
}

/// 加载的音频缓冲句柄。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferId(usize);

impl BufferId {
    pub fn null() -> Self {
        Self(usize::MAX)
    }
    pub fn is_null(&self) -> bool {
        self.0 == usize::MAX
    }
}

/// 音源句柄。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceId(usize);

impl SourceId {
    pub fn null() -> Self {
        Self(usize::MAX)
    }
    pub fn is_null(&self) -> bool {
        self.0 == usize::MAX
    }
}

/// WAV 文件简易解析器，提取 PCM 数据与格式。
#[cfg(feature = "openal")]
#[derive(Debug)]
struct WavData {
    sample_rate: i32,
    bits_per_sample: u16,
    channels: u16,
    data: Vec<u8>,
}

#[cfg(feature = "openal")]
impl WavData {
    fn parse(bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        if bytes.len() < 44 {
            return Err("wav: file too small".into());
        }
        // RIFF header
        if &bytes[0..4] != b"RIFF" {
            return Err("wav: missing RIFF header".into());
        }
        if &bytes[8..12] != b"WAVE" {
            return Err("wav: missing WAVE header".into());
        }
        // fmt chunk
        let mut fmt_pos = 12;
        let mut sample_rate: i32 = 0;
        let mut bits_per_sample: u16 = 0;
        let mut channels: u16 = 0;
        let mut data_offset = 0;
        let mut data_size = 0;

        while fmt_pos + 8 <= bytes.len() {
            let chunk_id = &bytes[fmt_pos..fmt_pos + 4];
            let chunk_size = u32::from_le_bytes(bytes[fmt_pos + 4..fmt_pos + 8].try_into()?) as usize;
            match chunk_id {
                b"fmt " => {
                    if fmt_pos + 8 + 16 <= bytes.len() {
                        let audio_format = u16::from_le_bytes(bytes[fmt_pos + 8..fmt_pos + 10].try_into()?);
                        if audio_format != 1 {
                            return Err(format!("wav: unsupported audio format {}", audio_format).into());
                        }
                        channels = u16::from_le_bytes(bytes[fmt_pos + 10..fmt_pos + 12].try_into()?);
                        sample_rate = i32::from_le_bytes(bytes[fmt_pos + 12..fmt_pos + 16].try_into()?);
                        bits_per_sample = u16::from_le_bytes(bytes[fmt_pos + 22..fmt_pos + 24].try_into()?);
                    }
                }
                b"data" => {
                    data_offset = fmt_pos + 8;
                    data_size = chunk_size;
                }
                _ => {}
            }
            fmt_pos += 8 + chunk_size;
            // chunk padding
            if chunk_size % 2 == 1 && fmt_pos < bytes.len() {
                fmt_pos += 1;
            }
        }

        if data_size == 0 {
            return Err("wav: no data chunk found".into());
        }

        let data = bytes[data_offset..data_offset + data_size].to_vec();

        Ok(WavData { sample_rate, bits_per_sample, channels, data })
    }

    fn al_format(&self) -> Result<i32, Box<dyn std::error::Error>> {
        match (self.channels, self.bits_per_sample) {
            (1, 8) => Ok(0x1100), // AL_FORMAT_MONO8
            (1, 16) => Ok(0x1101), // AL_FORMAT_MONO16
            (2, 8) => Ok(0x1102), // AL_FORMAT_STEREO8
            (2, 16) => Ok(0x1103), // AL_FORMAT_STEREO16
            _ => Err("wav: unsupported channel/bits combination".into()),
        }
    }
}

/// OpenAL 设备与上下文（RAII）。
#[cfg(feature = "openal")]
pub struct AudioContext {
    device: *mut openal_sys::ALCdevice,
    context: *mut openal_sys::ALCcontext,
}

#[cfg(feature = "openal")]
impl AudioContext {
    /// 打开默认 OpenAL 设备并创建上下文，设为当前线程当前。
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        unsafe {
            let device = openal_sys::alcOpenDevice(std::ptr::null());
            if device.is_null() {
                return Err("openal: alcOpenDevice failed".into());
            }
            let context = openal_sys::alcCreateContext(device, std::ptr::null());
            if context.is_null() {
                openal_sys::alcCloseDevice(device);
                return Err("openal: alcCreateContext failed".into());
            }
            if openal_sys::alcMakeContextCurrent(context) == 0 {
                openal_sys::alcDestroyContext(context);
                openal_sys::alcCloseDevice(device);
                return Err("openal: alcMakeContextCurrent failed".into());
            }
            // 检查 AL 错误
            let err = openal_sys::alGetError();
            if err != 0 {
                warn!("audio: OpenAL error after context creation: {err}");
            }
            info!("audio: initialized OpenAL device/context");
            Ok(AudioContext { device, context })
        }
    }
}

#[cfg(feature = "openal")]
impl Drop for AudioContext {
    fn drop(&mut self) {
        unsafe {
            if !self.context.is_null() {
                openal_sys::alcMakeContextCurrent(std::ptr::null_mut());
                openal_sys::alcDestroyContext(self.context);
            }
            if !self.device.is_null() {
                openal_sys::alcCloseDevice(self.device);
            }
        }
    }
}

/// 当 `openal` feature 未启用时的占位。
#[cfg(not(feature = "openal"))]
pub struct AudioContext;

#[cfg(not(feature = "openal"))]
impl AudioContext {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Err("deusex-audio: `openal` feature is disabled".into())
    }
}

/// 已加载的音频缓冲（OpenAL buffer）。
#[cfg(feature = "openal")]
struct AudioBuffer {
    id: BufferId,
    al_buffer: openal_sys::ALuint,
    sample_rate: i32,
    channels: u16,
    bits_per_sample: u16,
    duration_secs: f32,
}

/// 音源实例（OpenAL source）。
#[cfg(feature = "openal")]
struct AudioSource {
    id: SourceId,
    al_source: openal_sys::ALuint,
    buffer_id: Option<BufferId>,
    config: SourceConfig,
    state: SourceState,
}

/// 音频管理器：统一管理缓冲与音源。
#[cfg(feature = "openal")]
pub struct AudioManager {
    context: AudioContext,
    buffers: HashMap<BufferId, AudioBuffer>,
    sources: HashMap<SourceId, AudioSource>,
    next_buffer_id: usize,
    next_source_id: usize,
    listener: Listener,
    max_sources: usize,
}

/// 当 `openal` feature 未启用时的占位：无法加载或播放声音。
#[cfg(not(feature = "openal"))]
pub struct AudioManager;

#[cfg(not(feature = "openal"))]
impl AudioManager {
    pub fn new(_context: AudioContext) -> Result<Self, Box<dyn std::error::Error>> {
        Err("deusex-audio: `openal` feature is disabled".into())
    }
    pub fn load_buffer(&mut self, _name: &str, _bytes: &[u8]) -> Result<BufferId, Box<dyn std::error::Error>> {
        Err("deusex-audio: `openal` feature is disabled".into())
    }
    pub fn create_source(&mut self, _config: SourceConfig) -> Result<SourceId, Box<dyn std::error::Error>> {
        Err("deusex-audio: `openal` feature is disabled".into())
    }
    pub fn play(&mut self, _source: SourceId) -> Result<(), Box<dyn std::error::Error>> {
        Err("deusex-audio: `openal` feature is disabled".into())
    }
    pub fn stop(&mut self, _source: SourceId) -> Result<(), Box<dyn std::error::Error>> {
        Err("deusex-audio: `openal` feature is disabled".into())
    }
    pub fn pause(&mut self, _source: SourceId) -> Result<(), Box<dyn std::error::Error>> {
        Err("deusex-audio: `openal` feature is disabled".into())
    }
    pub fn set_listener(&mut self, _listener: Listener) {}
    pub fn update(&mut self) {}
}

#[cfg(feature = "openal")]
impl AudioManager {
    /// 创建音频管理器，绑定已初始化的 `AudioContext`。
    pub fn new(context: AudioContext) -> Result<Self, Box<dyn std::error::Error>> {
        let err = unsafe { openal_sys::alGetError() };
        if err != 0 {
            warn!("audio: OpenAL error before manager creation: {err}");
        }
        info!("audio: created AudioManager");
        Ok(AudioManager {
            context,
            buffers: HashMap::new(),
            sources: HashMap::new(),
            next_buffer_id: 0,
            next_source_id: 0,
            listener: Listener::default(),
            max_sources: 256,
        })
    }

    /// 从 WAV 字节数据加载音频缓冲。
    pub fn load_buffer(
        &mut self,
        name: &str,
        bytes: &[u8],
    ) -> Result<BufferId, Box<dyn std::error::Error>> {
        let wav = WavData::parse(bytes)?;
        let format = wav.al_format()?;

        let mut al_buffer: openal_sys::ALuint = 0;
        unsafe {
            openal_sys::alGenBuffers(1, &mut al_buffer);
            if al_buffer == 0 {
                return Err("openal: alGenBuffers failed".into());
            }
            openal_sys::alBufferData(
                al_buffer,
                format,
                wav.data.as_ptr() as *const std::ffi::c_void,
                wav.data.len() as openal_sys::ALsizei,
                wav.sample_rate as openal_sys::ALsizei,
            );
            let err = openal_sys::alGetError();
            if err != 0 {
                openal_sys::alDeleteBuffers(1, &al_buffer);
                return Err(format!("openal: alBufferData failed: {err}").into());
            }
        }

        let bits_per_sample = wav.bits_per_sample as f32;
        let channels = wav.channels as f32;
        let sample_rate = wav.sample_rate as f32;
        let duration_secs = (wav.data.len() as f32) / (sample_rate * channels * (bits_per_sample / 8.0));

        let id = BufferId(self.next_buffer_id);
        self.next_buffer_id += 1;
        self.buffers.insert(
            id,
            AudioBuffer {
                id,
                al_buffer,
                sample_rate: wav.sample_rate,
                channels: wav.channels,
                bits_per_sample: wav.bits_per_sample,
                duration_secs,
            },
        );

        debug!("audio: loaded buffer '{}' ({}ms, {}Hz, {}ch, {}bit)", name, duration_secs * 1000.0, wav.sample_rate, wav.channels, wav.bits_per_sample);
        Ok(id)
    }

    /// 创建一个音源。
    pub fn create_source(&mut self, config: SourceConfig) -> Result<SourceId, Box<dyn std::error::Error>> {
        let mut al_source: openal_sys::ALuint = 0;
        unsafe {
            openal_sys::alGenSources(1, &mut al_source);
            if al_source == 0 {
                return Err("openal: alGenSources failed".into());
            }
            self.apply_source_config(al_source, &config)?;
        }

        let id = SourceId(self.next_source_id);
        self.next_source_id += 1;
        self.sources.insert(
            id,
            AudioSource {
                id,
                al_source,
                buffer_id: None,
                config,
                state: SourceState::Initial,
            },
        );
        Ok(id)
    }

    /// 为音源绑定缓冲。
    pub fn bind_buffer(&mut self, source: SourceId, buffer: BufferId) -> Result<(), Box<dyn std::error::Error>> {
        let src = self.sources.get_mut(&source).ok_or("source not found")?;
        if !self.buffers.contains_key(&buffer) {
            return Err("buffer not found".into());
        }
        unsafe {
            openal_sys::alSourcei(src.al_source, openal_sys::AL_BUFFER, self.buffers[&buffer].al_buffer as openal_sys::ALint);
            let err = openal_sys::alGetError();
            if err != 0 {
                return Err(format!("openal: alSourcei AL_BUFFER failed: {err}").into());
            }
        }
        src.buffer_id = Some(buffer);
        Ok(())
    }

    /// 播放音源。
    pub fn play(&mut self, source: SourceId) -> Result<(), Box<dyn std::error::Error>> {
        let src = self.sources.get_mut(&source).ok_or("source not found")?;
        if src.buffer_id.is_none() {
            return Err("source has no buffer bound".into());
        }
        unsafe {
            openal_sys::alSourcePlay(src.al_source);
            let err = openal_sys::alGetError();
            if err != 0 {
                return Err(format!("openal: alSourcePlay failed: {err}").into());
            }
        }
        src.state = SourceState::Playing;
        Ok(())
    }

    /// 停止音源。
    pub fn stop(&mut self, source: SourceId) -> Result<(), Box<dyn std::error::Error>> {
        let src = self.sources.get_mut(&source).ok_or("source not found")?;
        unsafe {
            openal_sys::alSourceStop(src.al_source);
            let err = openal_sys::alGetError();
            if err != 0 {
                return Err(format!("openal: alSourceStop failed: {err}").into());
            }
        }
        src.state = SourceState::Stopped;
        Ok(())
    }

    /// 暂停音源。
    pub fn pause(&mut self, source: SourceId) -> Result<(), Box<dyn std::error::Error>> {
        let src = self.sources.get_mut(&source).ok_or("source not found")?;
        unsafe {
            openal_sys::alSourcePause(src.al_source);
            let err = openal_sys::alGetError();
            if err != 0 {
                return Err(format!("openal: alSourcePause failed: {err}").into());
            }
        }
        src.state = SourceState::Paused;
        Ok(())
    }

    /// 设置音源增益（音量，0.0~1.0）。
    pub fn set_gain(&mut self, source: SourceId, gain: f32) -> Result<(), Box<dyn std::error::Error>> {
        let src = self.sources.get_mut(&source).ok_or("source not found")?;
        unsafe {
            openal_sys::alSourcef(src.al_source, openal_sys::AL_GAIN, gain.clamp(0.0, 1.0) as openal_sys::ALfloat);
        }
        src.config.gain = gain.clamp(0.0, 1.0);
        Ok(())
    }

    /// 设置音源音调。
    pub fn set_pitch(&mut self, source: SourceId, pitch: f32) -> Result<(), Box<dyn std::error::Error>> {
        let src = self.sources.get_mut(&source).ok_or("source not found")?;
        unsafe {
            openal_sys::alSourcef(src.al_source, openal_sys::AL_PITCH, pitch.clamp(0.5, 2.0) as openal_sys::ALfloat);
        }
        src.config.pitch = pitch.clamp(0.5, 2.0);
        Ok(())
    }

    /// 设置音源 3D 位置。
    pub fn set_position(&mut self, source: SourceId, pos: Position) -> Result<(), Box<dyn std::error::Error>> {
        let src = self.sources.get_mut(&source).ok_or("source not found")?;
        unsafe {
            openal_sys::alSource3f(
                src.al_source,
                openal_sys::AL_POSITION,
                pos.x as openal_sys::ALfloat,
                pos.y as openal_sys::ALfloat,
                pos.z as openal_sys::ALfloat,
            );
        }
        src.config.position = pos;
        Ok(())
    }

    /// 设置是否循环。
    pub fn set_looping(&mut self, source: SourceId, looping: bool) -> Result<(), Box<dyn std::error::Error>> {
        let src = self.sources.get_mut(&source).ok_or("source not found")?;
        unsafe {
            openal_sys::alSourcei(
                src.al_source,
                openal_sys::AL_LOOPING,
                if looping { 1 } else { 0 },
            );
        }
        src.config.looping = looping;
        Ok(())
    }

    /// 获取音源当前状态。
    pub fn source_state(&mut self, source: SourceId) -> Result<SourceState, Box<dyn std::error::Error>> {
        let src = self.sources.get_mut(&source).ok_or("source not found")?;
        unsafe {
            let mut state: openal_sys::ALint = 0;
            openal_sys::alGetSourcei(src.al_source, openal_sys::AL_SOURCE_STATE, &mut state);
            let s = match state {
                openal_sys::AL_INITIAL => SourceState::Initial,
                openal_sys::AL_PLAYING => SourceState::Playing,
                openal_sys::AL_PAUSED => SourceState::Paused,
                openal_sys::AL_STOPPED => SourceState::Stopped,
                _ => SourceState::Stopped,
            };
            src.state = s;
            Ok(s)
        }
    }

    /// 设置音频监听者（玩家/摄像机）状态。
    pub fn set_listener(&mut self, listener: Listener) {
        unsafe {
            openal_sys::alListener3f(
                openal_sys::AL_POSITION,
                listener.position.x as openal_sys::ALfloat,
                listener.position.y as openal_sys::ALfloat,
                listener.position.z as openal_sys::ALfloat,
            );
            openal_sys::alListener3f(
                openal_sys::AL_VELOCITY,
                listener.velocity.x as openal_sys::ALfloat,
                listener.velocity.y as openal_sys::ALfloat,
                listener.velocity.z as openal_sys::ALfloat,
            );
            let orientation = [
                listener.forward.x as openal_sys::ALfloat,
                listener.forward.y as openal_sys::ALfloat,
                listener.forward.z as openal_sys::ALfloat,
                listener.up.x as openal_sys::ALfloat,
                listener.up.y as openal_sys::ALfloat,
                listener.up.z as openal_sys::ALfloat,
            ];
            openal_sys::alListenerfv(openal_sys::AL_ORIENTATION, orientation.as_ptr());
        }
        self.listener = listener;
    }

    /// 更新音频状态（每帧调用，清理已停止的非循环音源等）。
    pub fn update(&mut self) {
        // 刷新 AL 错误
        let _ = unsafe { openal_sys::alGetError() };
    }

    /// 返回当前监听者。
    pub fn listener(&self) -> Listener {
        self.listener
    }

    /// 返回缓冲数量。
    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    /// 返回音源数量。
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// 应用音源配置到 OpenAL。
    fn apply_source_config(
        &self,
        al_source: openal_sys::ALuint,
        config: &SourceConfig,
    ) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            openal_sys::alSourcef(al_source, openal_sys::AL_GAIN, config.gain.clamp(0.0, 1.0) as openal_sys::ALfloat);
            openal_sys::alSourcef(al_source, openal_sys::AL_PITCH, config.pitch.clamp(0.5, 2.0) as openal_sys::ALfloat);
            openal_sys::alSourcei(al_source, openal_sys::AL_LOOPING, if config.looping { 1 } else { 0 });
            openal_sys::alSource3f(
                al_source,
                openal_sys::AL_POSITION,
                config.position.x as openal_sys::ALfloat,
                config.position.y as openal_sys::ALfloat,
                config.position.z as openal_sys::ALfloat,
            );
            openal_sys::alSource3f(
                al_source,
                openal_sys::AL_VELOCITY,
                config.velocity.x as openal_sys::ALfloat,
                config.velocity.y as openal_sys::ALfloat,
                config.velocity.z as openal_sys::ALfloat,
            );
        }
        let err = unsafe { openal_sys::alGetError() };
        if err != 0 {
            return Err(format!("openal: apply_source_config failed: {err}").into());
        }
        Ok(())
    }
}

impl Drop for AudioManager {
    fn drop(&mut self) {
        #[cfg(feature = "openal")]
        {
            unsafe {
                for src in self.sources.values() {
                    openal_sys::alDeleteSources(1, &src.al_source);
                }
                for buf in self.buffers.values() {
                    openal_sys::alDeleteBuffers(1, &buf.al_buffer);
                }
            }
        }
    }
}

pub fn init() {
    info!("Initializing deusex-audio");
}
