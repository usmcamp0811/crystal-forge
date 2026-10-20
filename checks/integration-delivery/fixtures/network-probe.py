"""Serves controlled outside-network input without any external dependency."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import socket


class Probe(BaseHTTPRequestHandler):
    def do_GET(self):
        payload = b"CF-OWNED-OUTSIDE-PROBE\n"
        self.send_response(200)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


class Server(ThreadingHTTPServer):
    address_family = socket.AF_INET6

    def server_bind(self):
        self.socket.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 0)
        super().server_bind()


Server(("::", 9080), Probe).serve_forever()
