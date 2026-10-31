#pragma once

#include "types.hpp"
#include <cmath>

namespace smartpot::features {

#ifndef M_PI
#define M_PI 3.14159265358979323846
#endif

inline void normalize_features(
    const double* values,
    const SmartPotParamRange* ranges,
    int count,
    double* out_normalized
) noexcept {
    if (!values || !ranges || !out_normalized || count <= 0) {
        return;
    }
    for (int i = 0; i < count; ++i) {
        const double width = ranges[i].max_val - ranges[i].min_val;
        if (std::abs(width) > 1e-12) {
            out_normalized[i] = (values[i] - ranges[i].min_val) / width;
        } else {
            out_normalized[i] = 0.0;
        }
    }
}

inline void circular_hour_encoding(double hour, double* out_sin, double* out_cos) noexcept {
    if (!out_sin || !out_cos) return;
    const double angle = 2.0 * M_PI * hour / 24.0;
    *out_sin = std::sin(angle);
    *out_cos = std::cos(angle);
}

} // namespace smartpot::features
