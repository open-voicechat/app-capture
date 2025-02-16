use objc2::{define_class, msg_send, rc::Retained, runtime::ProtocolObject, AllocAnyThread, DefinedClass};
use objc2_core_graphics::CGMainDisplayID;
use objc2_foundation::{NSObject, NSObjectProtocol};
use objc2_screen_capture_kit::{SCContentFilter, SCContentSharingPicker, SCContentSharingPickerObserver, SCStream};

use crate::utils::callback_future::CallbackFuture;

use super::CaptureSource;

impl CaptureSource {
    pub fn pick() -> CallbackFuture<Option<Self>> {
        unsafe {
            CGMainDisplayID();
        };
        let fut = CallbackFuture::new();
        let picker = unsafe { SCContentSharingPicker::sharedPicker() };
        let observer = ProtocolObject::from_retained(PickerObserver::new(fut.clone()));
        unsafe { picker.setActive(true) };
        unsafe { picker.addObserver(&observer) };
        unsafe { picker.present() };
        fut
    }
}

#[derive(Clone)]
struct Ivars {
    object: Retained<NSObject>,
    fut: CallbackFuture<Option<CaptureSource>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "PickerObserver"]
    #[ivars = Ivars]
    struct PickerObserver;

    unsafe impl NSObjectProtocol for PickerObserver {}
    unsafe impl SCContentSharingPickerObserver for PickerObserver {
        #[unsafe(method(contentSharingPicker:didCancelForStream:))]
        fn content_sharing_picker_did_cancel_for_stream(
            &self,
            _picker: &SCContentSharingPicker,
            _stream: Option<&SCStream>,
        ) {
            println!("picker cancelled");
            self.ivars().fut.complete(None);
        }
        #[unsafe(method(contentSharingPicker:didUpdateWithFilter:forStream:))]
        unsafe fn content_sharing_picker_did_update_with_filter_for_stream(
            &self,
            _picker: &SCContentSharingPicker,
            filter: &SCContentFilter,
            _stream: Option<&SCStream>,
        ) {
            println!("picker updated");
            self.ivars().fut.complete(Some(CaptureSource {
                filter: filter.into(),
            }));
        }
    }
);

impl PickerObserver {
    fn new(fut: CallbackFuture<Option<CaptureSource>>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(Ivars {
            object: NSObject::new(),
            fut,
        });
        unsafe { msg_send![super(this), init] }
    }
}
