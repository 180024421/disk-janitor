//! 后台作业的统一生命周期：启动、取消、进度、结果接收与线程回收。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Scan,
    JunkScan,
    DuplicateScan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Running,
    Cancelling,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobProgress {
    pub completed: u64,
    pub total: Option<u64>,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobStatus {
    pub kind: JobKind,
    pub state: JobState,
    pub progress: Option<JobProgress>,
}

pub struct JobManager<M> {
    status: Option<JobStatus>,
    cancel: Option<Arc<AtomicBool>>,
    receiver: Option<Receiver<M>>,
    worker: Option<JoinHandle<()>>,
}

impl<M: Send + 'static> Default for JobManager<M> {
    fn default() -> Self {
        Self {
            status: None,
            cancel: None,
            receiver: None,
            worker: None,
        }
    }
}

impl<M: Send + 'static> JobManager<M> {
    pub fn start<F>(&mut self, kind: JobKind, work: F) -> Result<(), &'static str>
    where
        F: FnOnce(Arc<AtomicBool>, Sender<M>) + Send + 'static,
    {
        if self.is_running() {
            return Err("已有后台作业正在运行");
        }
        self.reap_worker();
        let cancel = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();
        let worker_cancel = Arc::clone(&cancel);
        self.worker = Some(std::thread::spawn(move || work(worker_cancel, sender)));
        self.cancel = Some(cancel);
        self.receiver = Some(receiver);
        self.status = Some(JobStatus {
            kind,
            state: JobState::Running,
            progress: None,
        });
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.status
            .as_ref()
            .is_some_and(|status| status.state != JobState::Finished)
    }

    pub fn status(&self) -> Option<&JobStatus> {
        self.status.as_ref()
    }

    pub fn worker_finished(&self) -> bool {
        self.worker.as_ref().is_some_and(JoinHandle::is_finished)
    }

    pub fn update_progress(&mut self, completed: u64, total: Option<u64>, label: String) {
        if let Some(status) = &mut self.status {
            status.progress = Some(JobProgress {
                completed,
                total,
                label,
            });
        }
    }

    pub fn cancel(&mut self) -> bool {
        let Some(cancel) = &self.cancel else {
            return false;
        };
        cancel.store(true, Ordering::Relaxed);
        if let Some(status) = &mut self.status {
            status.state = JobState::Cancelling;
        }
        true
    }

    pub fn drain(&self) -> Vec<M> {
        let Some(receiver) = &self.receiver else {
            return Vec::new();
        };
        receiver.try_iter().collect()
    }

    pub fn finish(&mut self) {
        if let Some(status) = &mut self.status {
            status.state = JobState::Finished;
        }
        self.receiver = None;
        self.cancel = None;
        self.reap_worker();
    }

    fn reap_worker(&mut self) {
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
}

impl<M> Drop for JobManager<M> {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        // 不在 UI 退出路径阻塞等待可能较慢的文件系统作业；JoinHandle drop 后线程自行收尾。
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn receives_result_and_tracks_lifecycle() {
        let mut jobs = JobManager::default();
        jobs.start(JobKind::Scan, |_cancel, tx| tx.send(42).unwrap())
            .unwrap();
        for _ in 0..50 {
            let messages = jobs.drain();
            if messages == [42] {
                jobs.finish();
                assert!(!jobs.is_running());
                assert_eq!(jobs.status().unwrap().state, JobState::Finished);
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("未收到后台作业结果");
    }

    #[test]
    fn cancellation_token_is_shared_with_worker() {
        let mut jobs = JobManager::default();
        jobs.start(JobKind::JunkScan, |cancel, tx| {
            while !cancel.load(Ordering::Relaxed) {
                std::thread::yield_now();
            }
            tx.send("cancelled").unwrap();
        })
        .unwrap();
        assert!(jobs.cancel());
        assert_eq!(jobs.status().unwrap().state, JobState::Cancelling);
    }
}
