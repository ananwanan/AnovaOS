#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Enter the pinned host build recipe without installing anything into the host sysroot.
set -eu
exec python3 "$(dirname "$0")/build.py"
