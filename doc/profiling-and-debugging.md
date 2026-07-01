# Whisperfish profiling and debugging

Whisperfish comes preloaded with Tracy, coz, and Tokio Console support,
but this is a compile-time option (the `tracy`, `coz` and `console-subscriber` feature flags respectively).
These feature flags are forwarded to RPM:

```
sfdk build --with tracy --with console_subscriber  # Mind the underscore!
sfdk build --enable-debug -- --with coz  # Mind the underscore!
```

All three are guarded by the runtime flag in `config.yml` named `tracing`; set this to true to enable Tokio Console and Tracy.
Note that a `coz` build disables all other logging layers, so coz is mutually exclusive with Tracy and console-subscriber.

Additionally, for coz, you need to install the coz dynamic library and libelfin, you can find them in my repo over at https://nas.rubdos.be/~rsmet/sailfish-repo/rpm/armv7hl/, built for armv7hl, or [build them from source with my RPM specs](https://github.com/rubdos/sfos-coz).
For building with coz support, you'll need the `{coz,libelfin}-devel` packages too.

To debug Signal websocket reconnection issues, you can manually kill the connection:

```bash
ss -K dst = '[2600:9000:a507:ab6d:4ce3:2f58:25d7:9cbf]'
```

## Diesel query instrumentation

Whisperfish can compile in a Diesel [connection instrumentation] hook that observes
every SQL statement run through `whisperfish-store`'s `SqliteConnection`. It is an
optional, debug-only feature:

```
sfdk build --with diesel_instrumentation   # sailfish device build
# or, on a host build:
cargo check -p whisperfish --features diesel-instrumentation
```

The hook lives in `whisperfish-store/src/store/instrumentation.rs` and is wired up
in `whisperfish-store/src/store.rs` behind `#[cfg(feature = "diesel-instrumentation")]`.
