#pragma once

#include "types.hpp"
#include <vector>
#include <cmath>
#include <algorithm>
#include <numeric>

namespace smartpot::quality {

[[nodiscard]] inline double compute_percentile(std::vector<double>& v, double p) noexcept {
    if (v.empty()) {
        return 0.0;
    }
    const size_t n = v.size();
    if (n == 1) {
        return v[0];
    }
    const double idx = p * (n - 1);
    const size_t lower = static_cast<size_t>(std::floor(idx));
    const size_t upper = static_cast<size_t>(std::ceil(idx));
    const double weight = idx - lower;

    std::nth_element(v.begin(), v.begin() + lower, v.end());
    double lower_val = v[lower];

    if (lower == upper) {
        return lower_val;
    }

    std::nth_element(v.begin(), v.begin() + upper, v.end());
    double upper_val = v[upper];

    return lower_val * (1.0 - weight) + upper_val * weight;
}

inline SmartPotQualityReport assess_data_quality(
    const double* matrix,
    int rows,
    int cols,
    const SmartPotPhysicalBound* bounds,
    uint8_t* out_keep_mask
) noexcept {
    SmartPotQualityReport report{};
    report.rows = rows;
    report.cols = cols;

    if (rows <= 0 || cols <= 0 || !matrix || !bounds) {
        report.score = 0.0;
        return report;
    }

    const size_t total_elements = static_cast<size_t>(rows) * cols;
    size_t non_nan_count = 0;
    size_t in_bounds_count = 0;

    // 1. Completeness y Validez Física
    std::vector<uint8_t> valid_matrix(total_elements, 0);

    for (int r = 0; r < rows; ++r) {
        for (int c = 0; c < cols; ++c) {
            const size_t idx = static_cast<size_t>(r) * cols + c;
            const double val = matrix[idx];

            if (!std::isnan(val) && !std::isinf(val)) {
                ++non_nan_count;
                if (val >= bounds[c].min_val && val <= bounds[c].max_val) {
                    ++in_bounds_count;
                    valid_matrix[idx] = 1;
                }
            }
        }
    }

    report.completeness = static_cast<double>(non_nan_count) / static_cast<double>(total_elements);
    report.validity = static_cast<double>(in_bounds_count) / static_cast<double>(total_elements);

    // 2. Outliers con IQR (k = 3.0) con dispersión mínima de 2% del rango físico
    std::vector<uint8_t> outlier_row(rows, 0);
    size_t total_outliers = 0;

    for (int c = 0; c < cols; ++c) {
        std::vector<double> col_values;
        col_values.reserve(rows);

        for (int r = 0; r < rows; ++r) {
            const size_t idx = static_cast<size_t>(r) * cols + c;
            const double val = matrix[idx];
            if (!std::isnan(val) && !std::isinf(val)) {
                col_values.push_back(val);
            }
        }

        if (col_values.size() >= 20) {
            const double q25 = compute_percentile(col_values, 0.25);
            const double q75 = compute_percentile(col_values, 0.75);
            const double physical_span = bounds[c].max_val - bounds[c].min_val;
            const double spread = std::max(q75 - q25, 0.02 * physical_span);
            const double lower_threshold = q25 - 3.0 * spread;
            const double upper_threshold = q75 + 3.0 * spread;

            for (int r = 0; r < rows; ++r) {
                const size_t idx = static_cast<size_t>(r) * cols + c;
                const double val = matrix[idx];
                if (!std::isnan(val) && !std::isinf(val)) {
                    if (val < lower_threshold || val > upper_threshold) {
                        outlier_row[r] = 1;
                    }
                }
            }
        }
    }

    for (int r = 0; r < rows; ++r) {
        if (outlier_row[r]) {
            ++total_outliers;
        }
    }
    report.outliers = static_cast<double>(total_outliers);

    // 3. Mascara de conservación (keep: fila completa en rango, sin NaN y sin outliers)
    int kept = 0;
    for (int r = 0; r < rows; ++r) {
        bool keep_row = (!outlier_row[r]);
        if (keep_row) {
            for (int c = 0; c < cols; ++c) {
                const size_t idx = static_cast<size_t>(r) * cols + c;
                if (!valid_matrix[idx]) {
                    keep_row = false;
                    break;
                }
            }
        }
        if (out_keep_mask) {
            out_keep_mask[r] = keep_row ? 1 : 0;
        }
        if (keep_row) {
            ++kept;
        }
    }
    report.keep_count = kept;

    // 4. Data Quality Score: completeness * validity * keep_ratio
    const double keep_ratio = static_cast<double>(kept) / static_cast<double>(rows);
    report.score = report.completeness * report.validity * keep_ratio;

    return report;
}

} // namespace smartpot::quality
