```sh
cd /home/ray/MEGA/Rays/Programming/rust/filesearcherV5-web
aider --model openai/gemma-4-31b --chat-mode ask

/add src/main.rs /Cargo.toml

aider --model openai/gemma-4-31b --lint-cmd "cargo check --message-format=short" --auto-lint
```

**vibe**
This is a web server written in rust using axum. It is for uploading and downloading files.

Currently the upload route is working fine.
Please add a new download route, that will download a file if found, based on header "file-path" which will contain a subdir and a filename. The downloaded file will be based on a locally set base directory, so subdirs will be under that directory.
