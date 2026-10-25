// SPDX-License-Identifier: Apache-2.0
// Provide initialized dynamic TLS accessed through musl's real shared-object relocation path.
static _Thread_local volatile int value=41;
int m9_shared(int add) {int before=value;value+=add;return before+add;}
