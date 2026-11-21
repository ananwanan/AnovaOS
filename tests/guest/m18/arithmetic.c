// SPDX-License-Identifier: Apache-2.0
// Compile and link all three inherited GCC arithmetic libraries in the guest.

#include <gmp.h>
#include <mpfr.h>
#include <mpc.h>
#include <stdio.h>

int main(void)
{
    mpz_t integer;
    mpfr_t real;
    mpc_t complex;

    mpz_init_set_ui(integer, 6);
    mpz_mul_ui(integer, integer, 7);
    mpfr_init2(real, 80);
    mpfr_set_z(real, integer, MPFR_RNDN);
    mpc_init2(complex, 80);
    mpc_set_ui(complex, 1, MPC_RNDNN);
    mpc_mul_i(complex, complex, 1, MPC_RNDNN);
    mpc_add_fr(complex, complex, real, MPC_RNDNN);

    int ok = mpz_cmp_ui(integer, 42) == 0 && mpfr_cmp_ui(real, 42) == 0
        && mpc_cmp_si_si(complex, 42, 1) == 0;
    mpc_clear(complex);
    mpfr_clear(real);
    mpz_clear(integer);
    if (!ok) {
        fputs("[M18:GCC:FAIL] unexpected arithmetic result\n", stderr);
        return 1;
    }
    puts("[M18:GCC:ARITHMETIC] gmp=42 mpfr=42 mpc=42+1i");
    return 0;
}
