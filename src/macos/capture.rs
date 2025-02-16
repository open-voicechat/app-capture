use core::slice;
use std::{cell::RefCell, ptr, time::SystemTime};

use block2::RcBlock;
use dispatch2::ffi::dispatch_queue_t;
use objc2::{define_class, ffi::objc_release, msg_send, rc::Retained, runtime::{AnyObject, ProtocolObject}, AllocAnyThread, DefinedClass};
use objc2_core_audio_types::{kAudioFormatLinearPCM, AudioBuffer, AudioBufferList};
use objc2_core_media::{kCMMediaType_Audio, CMAudioFormatDescriptionGetMostCompatibleFormat, CMFormatDescriptionGetMediaType, CMSampleBuffer, CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer, CMSampleBufferGetDuration, CMSampleBufferGetFormatDescription, CMSampleBufferGetImageBuffer, CMSampleBufferGetNumSamples, CMSampleBufferGetTotalSampleSize, CMTimeMake};
use objc2_core_video::{CVPixelBufferGetHeight, CVPixelBufferGetWidth};
use objc2_foundation::{NSError, NSObject, NSObjectProtocol};
use objc2_screen_capture_kit::{SCStream, SCStreamConfiguration, SCStreamDelegate, SCStreamOutput, SCStreamOutputType};

use crate::utils::callback_future::CallbackFuture;

use super::{AudioCallback, CaptureSession, CaptureSource};

impl CaptureSource {
    pub fn capture(&self, audio_callback: AudioCallback) -> CaptureSession {
        // create stream configuration
        let config = unsafe { SCStreamConfiguration::init(SCStreamConfiguration::alloc()) };

        unsafe {
            config.setCapturesAudio(true);
            config.setChannelCount(1);
            //config.setWidth(200);
            //config.setHeight(200);
            config.setQueueDepth(3);
            config.setMinimumFrameInterval(CMTimeMake(1, 3));
        }

        // configure stream
        let delegate = ProtocolObject::from_retained(MyStreamDelegate::new());
        let stream = unsafe {
            SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                &self.filter,
                &config,
                Some(&delegate),
            )
        };

        //let mut sample_handler_queue = Box::new(Queue::new(
        //    "sample_handler_queue",
        //    dispatch2::QueueAttribute::Serial,
        //));
        //sample_handler_queue.activate();

        let stream_output = ProtocolObject::from_retained(StreamOutput::new(audio_callback));

        add_stream_output(&stream, &stream_output, SCStreamOutputType::Screen, ptr::null_mut())
        .expect("shoulddddt work");

        add_stream_output(&stream, &stream_output, SCStreamOutputType::Audio, ptr::null_mut())
        .expect("shoulddddt work");

        CaptureSession {
            stream,
            stream_output,
            audio_callback,
        }
    }
}

impl CaptureSession {
    pub fn start(&self) -> CallbackFuture<()> {
        let fut = CallbackFuture::new();
        let fut_c = fut.clone();
        unsafe {
            self.stream.startCaptureWithCompletionHandler(Some(&RcBlock::new(move |e| {
                println!("startCaptureWithCompletionHandler err: {:?}", e);
                fut_c.complete(());
            })))
        }
        fut
    }
    pub async fn stop(&self) -> CallbackFuture<()> {
        let fut = CallbackFuture::new();
        let fut_c = fut.clone();
        unsafe {
            self.stream.stopCaptureWithCompletionHandler(Some(&RcBlock::new(move |e| {
                println!("stopCaptureWithCompletionHandler err: {:?}", e);
                fut_c.complete(());
            })))
        };
        fut
    }
}

fn add_stream_output(
    stream: &SCStream,
    output: &ProtocolObject<dyn SCStreamOutput>,
    r#type: SCStreamOutputType,
    sample_handler_queue: dispatch_queue_t,
) -> Result<(), Retained<NSError>> {
    unsafe {
        msg_send![stream, addStreamOutput: output, type: r#type, sampleHandlerQueue: sample_handler_queue, error: _]
    }
}

#[derive(Clone)]
struct Ivars {
    object: Retained<NSObject>,
    //data: RefCell<Vec<f32>>,
    audio_callback: AudioCallback,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "StreamOutput"]
    #[ivars = Ivars]
    struct StreamOutput;

    unsafe impl NSObjectProtocol for StreamOutput {}
    unsafe impl SCStreamOutput for StreamOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn stream_did_output_sample_buffer_of_type(
            &self,
            stream: &SCStream,
            sample_buffer: &CMSampleBuffer,
            r#type: SCStreamOutputType,
        ) {
            println!("DID OUTPUT");
            let samples = unsafe { CMSampleBufferGetNumSamples(sample_buffer) };
            let size = unsafe { CMSampleBufferGetTotalSampleSize(sample_buffer) };
            let _duration = unsafe { CMSampleBufferGetDuration(sample_buffer) };
            let t = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);

            if r#type == SCStreamOutputType::Screen {
                if let Some(image) = unsafe { CMSampleBufferGetImageBuffer(sample_buffer) } {
                    let w = unsafe { CVPixelBufferGetWidth(&image) };
                    let h = unsafe { CVPixelBufferGetHeight(&image) };
                    println!(
                        "[{:?}]Output! type={} samples={} bytes={} w={} h={}",
                        t, r#type.0, samples, size, w, h
                    );
                } else {
                    println!(
                        "[{:?}]Output! type={} samples={} bytes={}",
                        t, r#type.0, samples, size
                    );
                }
            } else if r#type == SCStreamOutputType::Audio {
                self.handle_audio(sample_buffer);
            }
        }
    }
);

impl StreamOutput {
    fn new(audio_callback: AudioCallback) -> Retained<Self> {
        let this = Self::alloc().set_ivars(Ivars {
            object: NSObject::new(),
            //data: RefCell::new(vec![]),
            audio_callback,
        });
        unsafe { msg_send![super(this), init] }
    }

    fn handle_audio(&self, sample_buffer: &CMSampleBuffer) {
        // get number of samples
        let samples = unsafe { CMSampleBufferGetNumSamples(sample_buffer) } as usize;

        // get format description
        let format_description = unsafe { CMSampleBufferGetFormatDescription(sample_buffer) }.unwrap();

        // validate that format description is audio
        let format_description_media_type = unsafe { CMFormatDescriptionGetMediaType(&format_description) };
        assert_eq!(format_description_media_type, kCMMediaType_Audio);

        // get audio format list
        let audio_format_list_item = unsafe { *CMAudioFormatDescriptionGetMostCompatibleFormat(&format_description) };
        let channel_layout_tag = audio_format_list_item.mChannelLayoutTag;
        let asbd = audio_format_list_item.mASBD;
        assert_eq!(asbd.mFormatID, kAudioFormatLinearPCM);

        // determine needed AudioBufferList size
        let mut needed_buffer_list_size = usize::default();
        let status = unsafe {
            CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
                sample_buffer,
                &mut needed_buffer_list_size,
                ptr::null_mut(),
                0,
                None,
                None,
                0,
                ptr::null_mut(),
            )
        };
        assert_eq!(status, 0);
        assert_eq!(needed_buffer_list_size, size_of::<AudioBufferList>());

        // get audio data
        let mut audio_buffer_list = AudioBufferList {
            mNumberBuffers: 1,
            mBuffers: [AudioBuffer {
                mNumberChannels: u32::default(),
                mDataByteSize: u32::default(),
                mData: ptr::null_mut(),
            }],
        };
        let mut block_buffer = ptr::null_mut();
        let status = unsafe {
            CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
                sample_buffer,
                ptr::null_mut(),
                &mut audio_buffer_list,
                size_of::<AudioBufferList>(),
                None,
                None,
                0,
                &mut block_buffer,
            )
        };
        assert_eq!(status, 0);
        let b = audio_buffer_list.mBuffers[0];
        println!(
            "[AUDIO] chLayout={} asbd(format={} rate={} bit/ch={} byte/f={} byte/p={}) abl(numChannels={} dataSize={} data={:?})",
            channel_layout_tag,
            asbd.mFormatID,
            asbd.mSampleRate,
            asbd.mBitsPerChannel,
            asbd.mBytesPerFrame,
            asbd.mBytesPerPacket,
            b.mNumberChannels, b.mDataByteSize, b.mData
        );

        unsafe { objc_release(block_buffer as *mut AnyObject) };

        // store audio data
        let ptr = b.mData as *mut f32;
        let data = unsafe { slice::from_raw_parts(ptr, samples) };
        (self.ivars().audio_callback)(data);

        //let mut data = self.ivars().data.borrow_mut();
        //for i in 0..samples {
        //    data.push(unsafe { ptr.add(i).read() });
        //}
    }
}

#[derive(Clone)]
struct Ivars2 {
    object: Retained<NSObject>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "MyStreamDelegate"]
    #[ivars = Ivars2]
    struct MyStreamDelegate;

    unsafe impl NSObjectProtocol for MyStreamDelegate {}

    unsafe impl SCStreamDelegate for MyStreamDelegate {
        #[unsafe(method(stream:didStopWithError:))]
        fn stream_did_stop_with_error(&self, stream: &SCStream, error: &NSError) {
            println!("stream did stop with error {:?}", error);
        }

        #[unsafe(method(outputVideoEffectDidStartForStream:))]
        fn output_video_effect_did_start_for_stream(&self, stream: &SCStream) {
            println!("output video effect did start");
        }

        #[unsafe(method(outputVideoEffectDidStopForStream:))]
        fn output_video_effect_did_stop_for_stream(&self, stream: &SCStream) {
            println!("output video effect did stop");
        }

        #[unsafe(method(streamDidBecomeActive:))]
        fn stream_did_become_active(&self, stream: &SCStream) {
            println!("stream did become active");
        }

        #[unsafe(method(streamDidBecomeInactive:))]
        fn stream_did_become_inactive(&self, stream: &SCStream) {
            println!("stream did become inactive");
        }
    }
);

impl MyStreamDelegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(Ivars2 {
            object: NSObject::new(),
        });
        unsafe { msg_send![super(this), init] }
    }
}

