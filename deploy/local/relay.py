#!/usr/bin/env python3
"""A local stand-in for the relay: a dumb TCP passthrough.

    deploy/local/relay.py <listen port> <Pi port>

Listens on 127.0.0.1:<listen port> and copies bytes, both ways, to the Pi on
127.0.0.1:<Pi port>. Like the real relay (rathole on the GCP VM) it never looks
inside: TLS ends on the Pi. It is not rathole, so it does not exercise the
Noise tunnel or the Pi's outbound dial; it exercises the path a device takes
(the relay's address in the invite, the Pi's pinned TLS key behind it).
"""
import asyncio
import sys


async def pipe(reader, writer):
    try:
        while data := await reader.read(65536):
            writer.write(data)
            await writer.drain()
    except (ConnectionError, asyncio.CancelledError):
        pass
    finally:
        writer.close()


async def handle(client_reader, client_writer, pi_port):
    try:
        pi_reader, pi_writer = await asyncio.open_connection("127.0.0.1", pi_port)
    except OSError as error:
        print(f"relay: the Pi is not reachable on 127.0.0.1:{pi_port}: {error}", flush=True)
        client_writer.close()
        return
    await asyncio.gather(
        pipe(client_reader, pi_writer),
        pipe(pi_reader, client_writer),
    )


async def main(listen_port, pi_port):
    server = await asyncio.start_server(
        lambda r, w: handle(r, w, pi_port), "127.0.0.1", listen_port
    )
    print(f"relay: 127.0.0.1:{listen_port} -> 127.0.0.1:{pi_port}", flush=True)
    async with server:
        await server.serve_forever()


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    try:
        asyncio.run(main(int(sys.argv[1]), int(sys.argv[2])))
    except KeyboardInterrupt:
        pass
