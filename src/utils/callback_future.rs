use std::{
    cell::Cell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[derive(Clone)]
pub struct CallbackFuture<T>(Rc<CallbackFutureInner<T>>);
struct CallbackFutureInner<T> {
    waker: Cell<Option<Waker>>,
    value: Cell<Option<T>>,
}

impl<T> CallbackFuture<T> {
    pub fn new() -> Self {
        Self(Rc::new(CallbackFutureInner {
            waker: Cell::new(None),
            value: Cell::new(None),
        }))
    }
    pub fn complete(&self, value: T) {
        self.0.value.set(Some(value));
        if let Some(w) = self.0.waker.take() {
            w.wake()
        }
    }
}

impl<T> Future for CallbackFuture<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.0.value.take() {
            Some(v) => Poll::Ready(v),
            None => {
                self.0.waker.set(Some(cx.waker().clone()));
                Poll::Pending
            }
        }
    }
}
