FSearch Qt6 Test Client

This test client talks to the `fsearch-core` Rust backend via a small C ABI.

Highlights JSON

- The `highlights` parameter delivered in the callback is a UTF-8 JSON string.
- It is either an array of highlight objects or an object mapping fields to ranges.
- Array object format: `{ "field": <string|null>, "ranges": [[start,end], ...] }`.
- `start`/`end` are UTF-16 code-unit indices (half-open) aligned to grapheme clusters.
- For field-scoped queries (e.g. `path:term`) the backend sets `field` to that field
  name (e.g. `"path"`). The client should prefer explicit `field` values.

Qt client notes

- The client receives callbacks from Rust worker threads and posts updates to
  the Qt main thread using `QMetaObject::invokeMethod(..., Qt::QueuedConnection)`.
- Ranges should be applied to `QString` using UTF-16 indices (e.g. `mid(start, len)`).

Build & Run

```bash
# from rust/qt-client
mkdir -p build && cd build
cmake .. && cmake --build .
./fsearch_qt_client
```
