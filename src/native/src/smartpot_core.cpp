#ifndef SMARTPOT_EXPORTS
#define SMARTPOT_EXPORTS
#endif
#include "smartpot/capi.h"
#include "smartpot/fuzzy.hpp"
#include "smartpot/forecast.hpp"
#include "smartpot/quality.hpp"
#include "smartpot/features.hpp"

#include <cstring>

static const char* ENGINE_VERSION = "SmartPot-AI Native Core 2.0 (C++20 SIMD)";

#if defined(__GNUC__) || defined(__clang__)
  #if defined(__AVX2__)
    static const char* BUILD_INFO = "GCC/Clang C++20 [AVX2+FMA Enabled, O3, Non-Alloc]";
  #elif defined(__SSE4_2__)
    static const char* BUILD_INFO = "GCC/Clang C++20 [SSE4.2 Enabled, O3, Non-Alloc]";
  #else
    static const char* BUILD_INFO = "GCC/Clang C++20 [Generic x86-64, O3, Non-Alloc]";
  #endif
#elif defined(_MSC_VER)
  static const char* BUILD_INFO = "MSVC C++20 [x64 Multi-threaded, O2, SIMD]";
#else
  static const char* BUILD_INFO = "Standard C++20 [Optimized]";
#endif

extern "C" {

SMARTPOT_API int smartpot_theil_sen(
    const double* hours,
    const double* values,
    int count,
    SmartPotTheilSenResult* out_result
) {
    if (!hours || !values || !out_result || count < 0) {
        return -1;
    }
    *out_result = smartpot::forecast::calculate_theil_sen(hours, values, static_cast<size_t>(count));
    return 0;
}

SMARTPOT_API int smartpot_fuzzy_health_index(
    const char* const* param_names,
    const double* deviations,
    int count,
    SmartPotHealthResult* out_result
) {
    if (!param_names || !deviations || !out_result || count < 0) {
        return -1;
    }
    *out_result = smartpot::fuzzy::compute_health_index(param_names, deviations, count);
    return 0;
}

SMARTPOT_API int smartpot_forecast_describe(
    const SmartPotParamRange* range,
    double current,
    double slope,
    double intercept,
    double agreement,
    SmartPotForecastResult* out_result
) {
    if (!range || !out_result) {
        return -1;
    }
    *out_result = smartpot::forecast::describe_forecast(*range, current, slope, intercept, agreement);
    return 0;
}

SMARTPOT_API int smartpot_quality_assess(
    const double* matrix,
    int rows,
    int cols,
    const SmartPotPhysicalBound* bounds,
    SmartPotQualityReport* out_report,
    uint8_t* out_keep_mask
) {
    if (!matrix || !bounds || !out_report || rows <= 0 || cols <= 0) {
        return -1;
    }
    *out_report = smartpot::quality::assess_data_quality(matrix, rows, cols, bounds, out_keep_mask);
    return 0;
}

SMARTPOT_API int smartpot_features_normalize(
    const double* values,
    const SmartPotParamRange* ranges,
    int count,
    double* out_normalized
) {
    if (!values || !ranges || !out_normalized || count <= 0) {
        return -1;
    }
    smartpot::features::normalize_features(values, ranges, count, out_normalized);
    return 0;
}

SMARTPOT_API int smartpot_features_circular_hour(
    double hour,
    double* out_sin,
    double* out_cos
) {
    if (!out_sin || !out_cos) {
        return -1;
    }
    smartpot::features::circular_hour_encoding(hour, out_sin, out_cos);
    return 0;
}

SMARTPOT_API const char* smartpot_engine_version(void) {
    return ENGINE_VERSION;
}

SMARTPOT_API const char* smartpot_engine_build_info(void) {
    return BUILD_INFO;
}

} // extern "C"
