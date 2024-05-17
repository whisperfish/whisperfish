mod common;

#[cfg(test)]
mod tests {
    use super::common::SimpleStorage;
    use std::fs;

    #[test]
    #[rustfmt::skip]
    fn log_rotation() {
        let temp_path = tempfile::tempdir().unwrap();
        let temp_pathbuf = temp_path.path().to_owned();

        fs::write(temp_pathbuf.join("dont-touch-me.tmp"), "ever").unwrap();

        // Same format as in harbour-whisperfish main.rs
        fs::write(temp_pathbuf.join("harbour-whisperfish.20001231_124500.log"), "log contents").unwrap();
        fs::write(temp_pathbuf.join("harbour-whisperfish.20001231_134500.log"), "log contents").unwrap();
        fs::write(temp_pathbuf.join("harbour-whisperfish.20001231_144500.log"), "log contents").unwrap();
        fs::write(temp_pathbuf.join("harbour-whisperfish.20001231_154500.log"), "log contents").unwrap();
        fs::write(temp_pathbuf.join("harbour-whisperfish.20001231_164500.log"), "log contents").unwrap();

        fs::write(temp_pathbuf.join("leave-me-alone.tmp"), "too").unwrap();

        // Same as in harbour-whisperfish main.rs
        const LOGFILE_REGEX: &str = r"harbour-whisperfish.\d{8}_\d{6}\.log";

        assert!(!SimpleStorage::clear_old_logs(&temp_pathbuf, 1, LOGFILE_REGEX));

        let nonexistent = temp_pathbuf.join("foobar");
        assert!(!SimpleStorage::clear_old_logs(&nonexistent, 5, LOGFILE_REGEX));

        assert!(SimpleStorage::clear_old_logs(&temp_pathbuf, 5, LOGFILE_REGEX));
        assert_eq!(fs::read_dir(&temp_pathbuf).unwrap().count(), 7);

        assert!(SimpleStorage::clear_old_logs(&temp_pathbuf, 3, LOGFILE_REGEX));
        assert_eq!(fs::read_dir(&temp_pathbuf).unwrap().count(), 5);

        let mut remaining: Vec<String> = fs::read_dir(&temp_pathbuf)
            .unwrap()
            .filter_map(|f| {
                if let Ok(f) = f {
                    Some(String::from(f.file_name().to_string_lossy()))
                } else {
                    None
                }
            })
            .collect();
        remaining.sort_by(|b, a| a.cmp(b));

        let mut files = remaining.iter();

        assert_eq!("leave-me-alone.tmp",                      files.next().unwrap());
        assert_eq!("harbour-whisperfish.20001231_164500.log", files.next().unwrap());
        assert_eq!("harbour-whisperfish.20001231_154500.log", files.next().unwrap());
        assert_eq!("harbour-whisperfish.20001231_144500.log", files.next().unwrap());
        // deleted: harbour-whisperfish.20001231_134500.log
        // deleted: harbour-whisperfish.20001231_124500.log
        assert_eq!("dont-touch-me.tmp",                       files.next().unwrap());
        assert!(files.next().is_none());

        drop(temp_path);
    }
}
