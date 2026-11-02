"""Read-only, loopback-only capture gallery with video byte-range support."""
import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import re


class ReviewHandler(SimpleHTTPRequestHandler):
    def list_directory(self, path):
        self.send_error(404)
        return None

    def send_head(self):
        path = Path(self.translate_path(self.path)).resolve()
        root = Path(self.directory).resolve()
        if not path.is_relative_to(root) or not path.is_file():
            self.send_error(404)
            return None
        if path.suffix.lower() not in {".html", ".png", ".jpg", ".jpeg", ".webp", ".mp4", ".webm", ".json"}:
            self.send_error(404)
            return None
        size = path.stat().st_size
        start, end = 0, size - 1
        byte_range = self.headers.get("Range")
        if byte_range:
            match = re.fullmatch(r"bytes=(\d*)-(\d*)", byte_range)
            if not match or not any(match.groups()):
                self.send_error(416)
                return None
            first, last = match.groups()
            if first:
                start = int(first)
                end = min(int(last), size - 1) if last else size - 1
            else:
                start = max(0, size - int(last))
            if start > end or start >= size:
                self.send_response(416)
                self.send_header("Content-Range", f"bytes */{size}")
                self.end_headers()
                return None
        handle = path.open("rb")
        handle.seek(start)
        self._remaining = end - start + 1
        self.send_response(206 if byte_range else 200)
        self.send_header("Content-Type", self.guess_type(str(path)))
        self.send_header("Content-Length", str(self._remaining))
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("X-Content-Type-Options", "nosniff")
        if byte_range:
            self.send_header("Content-Range", f"bytes {start}-{end}/{size}")
        self.end_headers()
        return handle

    def copyfile(self, source, outputfile):
        remaining = self._remaining
        try:
            while remaining > 0:
                chunk = source.read(min(256 * 1024, remaining))
                if not chunk:
                    break
                outputfile.write(chunk)
                remaining -= len(chunk)
        except (BrokenPipeError, ConnectionResetError):
            pass  # Browser changed clips or cancelled a seek.


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=8766)
    args = parser.parse_args()
    capture_root = Path(__file__).resolve().parents[1] / "captures"
    handler = partial(ReviewHandler, directory=str(capture_root))
    ThreadingHTTPServer(("127.0.0.1", args.port), handler).serve_forever()
