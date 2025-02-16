use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_screen_capture_kit::{SCContentFilter, SCStream, SCStreamOutput};

#[derive(Clone, Debug)]
pub struct CaptureSource {
    filter: Retained<SCContentFilter>,
}

#[derive(Clone, Debug)]
pub struct CaptureSession {
    stream: Retained<SCStream>,
    stream_output: Retained<ProtocolObject<dyn SCStreamOutput>>,
    audio_callback: AudioCallback,
}

type AudioCallback = fn(&[f32]) -> ();

pub mod pick;
pub mod capture;
