use std::sync::mpsc::{sync_channel, SyncSender, Receiver};
use std::sync::{Arc, Mutex, MutexGuard};
use futures::future::BoxFuture;
use futures::{Future, FutureExt, Poll};
use futures::task::{ArcWake, waker_ref, WakerRef, Context, Waker};
use std::time::Duration;
use std::thread;
use std::pin::Pin;

struct Task {
    future: Mutex<Option<BoxFuture<'static, ()>>>,

    // Handle to place task itself to executor's queue
    task_sender: SyncSender<Arc<Task>>,
}

impl ArcWake for Task {
    fn wake_by_ref(arc_self: &Arc<Task>) {
        // Implement wake by sending itself to the tasks queue
        // so that it will be polled again by an executor
        println!("task waked");
        let cloned = arc_self.clone();
        arc_self.task_sender.send(cloned).expect("too many requests");
    }
}

struct Spawner {
    tasks_sender: SyncSender<Arc<Task>>,
}

impl Spawner {
    fn spawn(&self, future: impl Future<Output = ()> + 'static + Send) {
        let future = future.boxed();
        let task = Arc::new(Task {
            future: Mutex::new(Some(future)),
            task_sender: self.tasks_sender.clone(),
        });
        self.tasks_sender.send(task);
    }
}

struct Executor {
    task_receiver: Receiver<Arc<Task>>,
}

impl Executor {
    fn run(&self) {
        loop {
            println!("executor iteration");
            let task = self.task_receiver.recv();
            match task {
                Ok(mut t) => {
                    println!("task received");
                    let mut future_slot: MutexGuard<_> = t.future.lock().unwrap();
                    match future_slot.take() {
                        Some(mut fut) => {
                            let waker: WakerRef = waker_ref(&t);
                            let mut ctx: Context = Context::from_waker(&*waker);
                            let poll_res = fut.as_mut().poll(&mut ctx);
                            match poll_res {
                                Poll::Pending => {
                                    println!("future pending");
                                    *future_slot = Some(fut);
                                }
                                Poll::Ready(_) => {
                                    println!("future done");
                                    ()
                                }
                            }
                        }
                        None => break,
                    }
                }
                Err(e) => {
                    println!("finish executor: {:?}", e);
                    break;
                }
            }
        }
    }
}

fn spawner_and_executor() -> (Spawner, Executor) {
    const MAX_QUEUED_TASKS: usize = 10_000;
    let (sender, receiver): (SyncSender<Arc<Task>>, Receiver<Arc<Task>>) = sync_channel(MAX_QUEUED_TASKS);

    (Spawner { tasks_sender: sender }, Executor { task_receiver: receiver })
}

struct TimedFuture {
    shared_state: Arc<Mutex<SharedState>>,
}

struct SharedState {
    completed: bool,
    waker: Option<Waker>,
}

impl TimedFuture {
    fn new(duration: Duration) -> TimedFuture {
        let shared_state = Arc::new(Mutex::new(SharedState {
            completed: false,
            waker: None,
        }));

        let thread_shared_state = shared_state.clone();
        thread::spawn(move || {
            println!("timed future started. sleep..");
            thread::sleep(duration);
            println!("sleep finish");
            let mut shared_state = thread_shared_state.lock().unwrap();
            shared_state.completed = true;
            if let Some(waker) = shared_state.waker.take() {
                println!("call wake");
                waker.wake();
            }
        });

        TimedFuture { shared_state }
    }
}

impl Future for TimedFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        println!("poll..");
        let mut share_state = self.shared_state.lock().unwrap();
        if share_state.completed {
            Poll::Ready(())
        } else {
            share_state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

fn main() {
    let (spawner, executor) = spawner_and_executor();
    let fut = async {
        println!("Hello");
        TimedFuture::new(Duration::from_secs(10)).await;
        println!("Bue");
    };
    spawner.spawn(fut);

    // Drop spawner to not hanging in cycle
    // after drop the executor will know that no tasks will appear in tasks queue
    drop(spawner);
    executor.run();
}
