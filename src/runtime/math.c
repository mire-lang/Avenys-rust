#include "runtime.h"
#define _GNU_SOURCE
#include <math.h>
#include <stdint.h>
#include <stdlib.h>
#include <float.h>

double rt_math_pi(void) { return 3.14159265358979323846; }
double rt_math_e(void) { return 2.71828182845904523536; }
double rt_math_tau(void) { return 6.28318530717958647692; }

double rt_math_sin(double value) { return sin(value); }
double rt_math_cos(double value) { return cos(value); }
double rt_math_tan(double value) { return tan(value); }
double rt_math_sqrt(double value) { return sqrt(value); }
double rt_math_pow(double base, double exponent) { return pow(base, exponent); }
double rt_math_log(double value) { return log(value); }
double rt_math_log10(double value) { return log10(value); }
double rt_math_exp(double value) { return exp(value); }
double rt_math_atan2(double y, double x) { return atan2(y, x); }
double rt_math_asin(double value) { return asin(value); }
double rt_math_acos(double value) { return acos(value); }
double rt_math_atan(double value) { return atan(value); }

int64_t rt_math_round(double value) { return (int64_t)llround(value); }
int64_t rt_math_floor(double value) { return (int64_t)floor(value); }
int64_t rt_math_ceil(double value) { return (int64_t)ceil(value); }
double rt_math_trunc(double value) { return trunc(value); }
double rt_math_fabs(double value) { return fabs(value); }
double rt_math_fmod(double x, double y) { return fmod(x, y); }
double rt_math_remainder(double x, double y) { return remainder(x, y); }
double rt_math_fma(double x, double y, double z) { return fma(x, y, z); }
double rt_math_copysign(double x, double y) { return copysign(x, y); }
double rt_math_frexp(double value, int *exponent) { return frexp(value, exponent); }
double rt_math_ldexp(double value, int i) { return ldexp(value, i); }
double rt_math_nextafter(double x, double y) { return nextafter(x, y); }
double rt_math_ulp(double value) { return nextafter(value, INFINITY) - value; }
double rt_math_modf(double value, double *iptr) {
    double intpart;
    double frac = modf(value, &intpart);
    *iptr = intpart;
    return frac;
}

int64_t rt_math_isfinite(double value) { return isfinite(value); }
int64_t rt_math_isinf(double value) { return isinf(value); }
int64_t rt_math_isnan(double value) { return isnan(value); }
int64_t rt_math_isclose(double a, double b, double rel_tol, double abs_tol) {
    double diff = fabs(a - b);
    if (diff <= abs_tol) return 1;
    if (diff <= rel_tol * fmax(fabs(a), fabs(b))) return 1;
    return 0;
}

double rt_math_cbrt(double value) { return cbrt(value); }
double rt_math_exp2(double value) { return exp2(value); }
double rt_math_expm1(double value) { return expm1(value); }
double rt_math_log2(double value) { return log2(value); }
double rt_math_log1p(double value) { return log1p(value); }

double rt_math_sinh(double value) { return sinh(value); }
double rt_math_cosh(double value) { return cosh(value); }
double rt_math_tanh(double value) { return tanh(value); }
double rt_math_asinh(double value) { return asinh(value); }
double rt_math_acosh(double value) { return acosh(value); }
double rt_math_atanh(double value) { return atanh(value); }

double rt_math_erf(double value) { return erf(value); }
double rt_math_erfc(double value) { return erfc(value); }
double rt_math_gamma(double value) { return tgamma(value); }
double rt_math_lgamma(double value) { return lgamma(value); }

double rt_math_degrees(double value) { return value * 180.0 / rt_math_pi(); }
double rt_math_radians(double value) { return value * rt_math_pi() / 180.0; }

double rt_math_inf(void) { return INFINITY; }
double rt_math_neg_inf(void) { return -INFINITY; }
double rt_math_nan(void) { return NAN; }
double rt_math_epsilon(void) { return DBL_EPSILON; }

static int64_t gcd_i64(int64_t a, int64_t b) {
    while (b != 0) {
        int64_t t = b;
        b = a % b;
        a = t;
    }
    return a >= 0 ? a : -a;
}

static int64_t lcm_i64(int64_t a, int64_t b) {
    if (a == 0 || b == 0) return 0;
    int64_t g = gcd_i64(a, b);
    return (a / g) * b;
}

static int64_t factorial_i64(int64_t n) {
    int64_t result = 1;
    for (int64_t i = 2; i <= n; i++) {
        result *= i;
    }
    return result;
}

int64_t rt_math_gcd(int64_t a, int64_t b) { return gcd_i64(a, b); }
int64_t rt_math_lcm(int64_t a, int64_t b) { return lcm_i64(a, b); }
int64_t rt_math_factorial(int64_t n) {
    if (n < 0) return 0;
    return factorial_i64(n);
}
int64_t rt_math_isqrt(int64_t n) {
    if (n < 0) return 0;
    return (int64_t)sqrt((double)n);
}
int64_t rt_math_perm(int64_t n, int64_t k) {
    if (k < 0 || k > n) return 0;
    return factorial_i64(n) / factorial_i64(n - k);
}
int64_t rt_math_comb(int64_t n, int64_t k) {
    if (k < 0 || k > n) return 0;
    if (k > n - k) k = n - k;
    int64_t result = 1;
    for (int64_t i = 1; i <= k; i++) {
        result = result * (n - k + i) / i;
    }
    return result;
}

double rt_math_hypot(double x, double y) { return hypot(x, y); }

double rt_math_dist(void *p, void *q) {
    int64_t len_p = rt_list_len(p);
    int64_t len_q = rt_list_len(q);
    if (len_p != len_q || len_p <= 0) return NAN;
    double sum = 0.0;
    for (int64_t i = 0; i < len_p; i++) {
        double pi = rt_read_f64((char *)p + i * 8);
        double qi = rt_read_f64((char *)q + i * 8);
        double diff = pi - qi;
        sum += diff * diff;
    }
    return sqrt(sum);
}

double rt_math_fsum(void *list) {
    int64_t len = rt_list_len(list);
    if (len <= 0) return 0.0;
    double sum = 0.0;
    double c = 0.0;
    for (int64_t i = 0; i < len; i++) {
        double y = rt_read_f64((char *)list + i * 8) - c;
        double t = sum + y;
        c = (t - sum) - y;
        sum = t;
    }
    return sum;
}

double rt_math_prod(void *list, double start) {
    int64_t len = rt_list_len(list);
    double result = start;
    for (int64_t i = 0; i < len; i++) {
        result *= rt_read_f64((char *)list + i * 8);
    }
    return result;
}

double rt_math_sumprod(void *p, void *q) {
    int64_t len_p = rt_list_len(p);
    int64_t len_q = rt_list_len(q);
    if (len_p != len_q || len_p <= 0) return 0.0;
    double sum = 0.0;
    for (int64_t i = 0; i < len_p; i++) {
        double pi = rt_read_f64((char *)p + i * 8);
        double qi = rt_read_f64((char *)q + i * 8);
        sum += pi * qi;
    }
    return sum;
}

int64_t rt_math_sum_i64(void *list) {
    int64_t len = rt_list_len(list);
    int64_t total = 0;
    for (int64_t i = 0; i < len; i++) {
        total += rt_list_get_i64(list, i);
    }
    return total;
}

int64_t rt_math_min_list_i64(void *list) {
    int64_t len = rt_list_len(list);
    if (len <= 0) return 0;
    int64_t result = rt_list_get_i64(list, 0);
    for (int64_t i = 1; i < len; i++) {
        int64_t value = rt_list_get_i64(list, i);
        if (value < result) result = value;
    }
    return result;
}

int64_t rt_math_max_list_i64(void *list) {
    int64_t len = rt_list_len(list);
    if (len <= 0) return 0;
    int64_t result = rt_list_get_i64(list, 0);
    for (int64_t i = 1; i < len; i++) {
        int64_t value = rt_list_get_i64(list, i);
        if (value > result) result = value;
    }
    return result;
}

double rt_math_mean_i64(void *list) {
    int64_t len = rt_list_len(list);
    if (len <= 0) return 0.0;
    return (double)rt_math_sum_i64(list) / (double)len;
}

double rt_math_variance_i64(void *list) {
    int64_t len = rt_list_len(list);
    if (len <= 0) return 0.0;
    double mean = rt_math_mean_i64(list);
    double total = 0.0;
    for (int64_t i = 0; i < len; i++) {
        double delta = (double)rt_list_get_i64(list, i) - mean;
        total += delta * delta;
    }
    return total / (double)len;
}

double rt_math_stddev_i64(void *list) {
    return sqrt(rt_math_variance_i64(list));
}

static void sort_i64(int64_t *values, int64_t len) {
    for (int64_t i = 1; i < len; i++) {
        int64_t key = values[i];
        int64_t j = i - 1;
        while (j >= 0 && values[j] > key) {
            values[j + 1] = values[j];
            j--;
        }
        values[j + 1] = key;
    }
}

double rt_math_median_i64(void *list) {
    int64_t len = rt_list_len(list);
    if (len <= 0) return 0.0;
    int64_t *copy = (int64_t *)malloc((size_t)len * sizeof(int64_t));
    if (!copy) return 0.0;
    for (int64_t i = 0; i < len; i++) {
        copy[i] = rt_list_get_i64(list, i);
    }
    sort_i64(copy, len);
    double result;
    if ((len & 1) == 1) {
        result = (double)copy[len / 2];
    } else {
        result = ((double)copy[(len / 2) - 1] + (double)copy[len / 2]) / 2.0;
    }
    free(copy);
    return result;
}

void *rt_math_range_i64(int64_t end) {
    return rt_math_range_step_i64(0, end, 1);
}

void *rt_math_range_between_i64(int64_t start, int64_t end) {
    return rt_math_range_step_i64(start, end, start <= end ? 1 : -1);
}

void *rt_math_range_step_i64(int64_t start, int64_t end, int64_t step) {
    void *result = rt_list_create(8, 8);
    if (!result || step == 0) return result;
    if (step > 0) {
        for (int64_t value = start; value < end; value += step) {
            result = rt_list_push_i64(result, value);
        }
    } else {
        for (int64_t value = start; value > end; value += step) {
            result = rt_list_push_i64(result, value);
        }
    }
    return result;
}
