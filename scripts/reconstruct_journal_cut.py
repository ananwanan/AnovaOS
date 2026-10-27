# SPDX-License-Identifier: Apache-2.0
# Reconstruct any retained host journal fault from its exact baseline, sector bytes and zero-based cut event.
"""Write a new regular image under build/; never modify the baseline or a real block device."""
import argparse
from pathlib import Path


def reconstruct(baseline, events, cut, cache=False, tear=False):
    media = bytearray(baseline)
    pending = []
    lines = events.splitlines()
    if cut < 0 or cut >= len(lines):
        raise ValueError("Cut event is outside the retained trace")
    for index, line in enumerate(lines):
        fields = line.split()
        if fields == ["flush"]:
            if index == cut:
                break
            for offset, data in pending:
                media[offset:offset + 512] = data
            pending.clear()
        elif len(fields) == 3 and fields[0] == "write":
            offset = int(fields[1]) * 512
            data = bytes.fromhex(fields[2])
            if offset < 0 or offset + 512 > len(media) or len(data) != 512:
                raise ValueError("Invalid recorded sector")
            if index == cut:
                if tear:
                    media[offset:offset + 256] = data[:256]
                break
            if cache:
                pending.append((offset, data))
            else:
                media[offset:offset + 512] = data
        else:
            raise ValueError("Invalid journal event trace")
    return media


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("events", type=Path)
    parser.add_argument("cut", type=int)
    parser.add_argument("output", type=Path)
    parser.add_argument("--cache", action="store_true")
    parser.add_argument("--tear", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1] / "build"
    if not args.output.resolve().is_relative_to(root.resolve()):
        parser.error("Output must be a new regular image under build/")
    data = reconstruct(args.baseline.read_bytes(), args.events.read_text(), args.cut, args.cache, args.tear)
    with args.output.open("xb") as stream:
        stream.write(data)


if __name__ == "__main__":
    main()
