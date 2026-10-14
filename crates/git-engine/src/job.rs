//! Owning the whole process tree of a long-running git command, so cancelling it really stops
//! everything git started: `git-remote-https`, `ssh`, `git-lfs`, and hooks run through Git for
//! Windows' MSYS `sh` (`git lfs pre-push`, `sleep`, …).
//!
//! # Windows: Job Object
//! `taskkill /T` walks parent PIDs, but MSYS processes re-exec themselves and their Windows
//! parent chain is broken, so hook children survived a cancel. A Job Object does not depend on
//! PIDs: every process created by a member of the job joins the job automatically.
//!
//! - The job is created with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and only this module holds its
//!   handle (non-inheritable), so closing it terminates every process still in the job.
//! - Only standard, documented Job Object APIs are used, through the safe `win32job` wrapper
//!   (`CreateJobObjectW`, `SetInformationJobObject`, `AssignProcessToJobObject`,
//!   `CloseHandle`): no `unsafe` in this crate, no suspended processes, no thread manipulation.
//!
//! ## Race between spawn and assignment
//! `std::process::Command` cannot create a process directly inside a job on stable Rust
//! (`PROC_THREAD_ATTRIBUTE_JOB_LIST` needs the unstable `spawn_with_attributes`), and creating
//! it suspended would require resuming its main thread by hand (the classic pattern of process
//! hollowing, which we do not want to resemble). So the child is assigned right after `spawn`
//! returns. The window is a few microseconds, while git needs milliseconds of start-up
//! (loading DLLs, reading config) before it can start any subprocess; children created after
//! the assignment are in the job no matter how deep. As a last resort the direct child is also
//! killed with `Child::kill`. If the job cannot be created (very old Windows, or a parent job
//! forbidding nesting), we fall back to `Child::kill` alone.
//!
//! ## Normal exit
//! When git finished by itself, the kill-on-close limit is cleared before the handle is closed:
//! something git deliberately left running in the background (`git maintenance --auto`, a
//! detached `gc`) must not be killed just because the command returned.
//!
//! # Other platforms
//! Behaviour is unchanged: `Child::kill` on the direct child.

use std::process::Child;

/// Owns the process tree of one spawned command.
#[derive(Debug)]
pub(crate) struct ProcessTree {
    #[cfg(windows)]
    job: Option<win32job::Job>,
}

impl ProcessTree {
    /// Puts `child` (and everything it will start) under our control. Call right after spawn.
    pub(crate) fn attach(child: &Child) -> Self {
        #[cfg(windows)]
        {
            let job = windows_job(child)
                .map_err(|e| eprintln!("job object unavailable, cancel kills only git: {e}"))
                .ok();
            Self { job }
        }
        #[cfg(not(windows))]
        {
            let _ = child;
            Self {}
        }
    }

    /// Whether descendants are tracked (a Windows job is active).
    #[cfg(test)]
    pub(crate) fn tracks_descendants(&self) -> bool {
        #[cfg(windows)]
        {
            self.job.is_some()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    /// Kills `child` and every descendant, then reaps `child`.
    pub(crate) fn kill(self, child: &mut Child) {
        #[cfg(windows)]
        {
            // Closing the only handle of a kill-on-close job terminates all its processes.
            drop(self.job);
        }
        let _ = child.kill();
        let _ = child.wait();
    }

    /// The command finished on its own: stop tracking without killing what it left running.
    pub(crate) fn release(self) {
        #[cfg(windows)]
        if let Some(job) = self.job {
            let _ = job.set_extended_limit_info(&win32job::ExtendedLimitInfo::new());
            drop(job);
        }
    }
}

#[cfg(windows)]
fn windows_job(child: &Child) -> Result<win32job::Job, win32job::JobError> {
    use std::os::windows::io::AsRawHandle;

    let mut info = win32job::ExtendedLimitInfo::new();
    info.limit_kill_on_job_close();
    let job = win32job::Job::create_with_limit_info(&info)?;
    // The handle stays owned by `child`; the job only references the process.
    job.assign_process(child.as_raw_handle() as isize)?;
    Ok(job)
}
