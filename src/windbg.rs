use std::{
    ptr::null_mut,
    sync::{Arc, Mutex},
};

use tokio::sync::{mpsc, oneshot};
use windows::{
    Win32::System::Diagnostics::Debug::Extensions::{
        DEBUG_EXECUTE_ECHO, DEBUG_INTERRUPT_ACTIVE, DEBUG_OUTCTL_ALL_CLIENTS, DebugConnectWide,
        IDebugClient5, IDebugControl4, IDebugOutputCallbacksWide, IDebugOutputCallbacksWide_Impl,
    },
    core::{ComObject, HSTRING, Interface as _, PCWSTR, implement},
};

enum Request {
    Command(String, oneshot::Sender<windows::core::Result<String>>),
    BreakProgram(oneshot::Sender<windows::core::Result<()>>),
}

#[derive(Clone)]
pub struct DebuggerClient {
    send: mpsc::Sender<Request>,
    join_handle: Arc<Mutex<std::thread::JoinHandle<()>>>,
}

#[implement(IDebugOutputCallbacksWide)]
#[derive(Debug)]
struct OutputCapture {
    buffer: Arc<Mutex<String>>,
}

impl OutputCapture {
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(String::new())),
        }
    }

    pub fn get_output(self) -> String {
        Arc::try_unwrap(self.buffer).unwrap().into_inner().unwrap()
    }
}

impl IDebugOutputCallbacksWide_Impl for OutputCapture_Impl {
    fn Output(&self, _mask: u32, text: &PCWSTR) -> windows::core::Result<()> {
        let s = unsafe { text.to_string() }.unwrap();
        let mut buf = self.buffer.lock().unwrap();
        buf.push_str(&s);
        Ok(())
    }
}

impl DebuggerClient {
    pub async fn new(conn: String) -> windows::core::Result<Self> {
        let (tx, rx) = oneshot::channel();
        let thread = std::thread::spawn(|| {
            let client = unsafe {
                let mut client = null_mut();
                let remote = HSTRING::from(conn);
                let result = DebugConnectWide(&remote, &IDebugClient5::IID, &mut client);
                match result {
                    Ok(_) => (),
                    Err(e) => {
                        tx.send(Err(e)).unwrap();
                        return;
                    }
                }
                IDebugClient5::from_raw(client)
            };
            let control = client.cast::<IDebugControl4>();
            let control = match control {
                Ok(c) => c,
                Err(e) => {
                    tx.send(Err(e)).unwrap();
                    return;
                }
            };
            let (tx2, mut rx) = mpsc::channel(1);
            tx.send(Ok(tx2)).unwrap();
            while let Some(r) = rx.blocking_recv() {
                match r {
                    Request::Command(command, ret) => unsafe {
                        let command = HSTRING::from(command);
                        let capture = OutputCapture::new();
                        let capture_obj = ComObject::new(capture);
                        let res = client.SetOutputCallbacksWide(
                            &capture_obj.cast::<IDebugOutputCallbacksWide>().unwrap(),
                        );
                        if let Err(e) = res {
                            ret.send(Err(e)).unwrap();
                            continue;
                        }
                        let result = control.ExecuteWide(
                            DEBUG_OUTCTL_ALL_CLIENTS,
                            &command,
                            DEBUG_EXECUTE_ECHO,
                        );
                        let res = client.SetOutputCallbacksWide(None);
                        if let Err(e) = res {
                            ret.send(Err(e)).unwrap();
                            continue;
                        }
                        let capture = capture_obj.take().unwrap();
                        let mut capture = capture.get_output();
                        if let Err(e) = result {
                            use std::fmt::Write;
                            write!(capture, "\nCommand execution failed with Error: {e}").unwrap();
                        }
                        ret.send(Ok(capture)).unwrap();
                    },
                    Request::BreakProgram(ret) => unsafe {
                        ret.send(control.SetInterrupt(DEBUG_INTERRUPT_ACTIVE))
                            .unwrap();
                    },
                }
            }
        });
        let tx = rx.await.unwrap()?;
        Ok(Self {
            send: tx,
            join_handle: Arc::new(Mutex::new(thread)),
        })
    }

    pub async fn execute_command(&self, command: String) -> windows::core::Result<String> {
        let (tx, rx) = oneshot::channel();
        self.send.send(Request::Command(command, tx)).await.unwrap();
        rx.await.unwrap()
    }

    pub async fn break_program(&self) -> windows::core::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send.send(Request::BreakProgram(tx)).await.unwrap();
        rx.await.unwrap()
    }

    pub fn close(self) {
        drop(self.send);
        let join_handle = Arc::try_unwrap(self.join_handle).unwrap();
        let join_handle = join_handle.into_inner().unwrap();
        join_handle.join().unwrap();
    }
}
