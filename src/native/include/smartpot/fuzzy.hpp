#pragma once

#include "types.hpp"
#include <string>
#include <string_view>
#include <vector>
#include <cmath>
#include <algorithm>
#include <cstring>

namespace smartpot::fuzzy {

struct TrapezoidTerm {
    double a;
    double b;
    double c;
    double d;
};

inline constexpr double OUTPUT_IDEAL = 100.0;
inline constexpr double OUTPUT_ACCEPTABLE = 75.0;
inline constexpr double OUTPUT_DEVIATED = 45.0;
inline constexpr double OUTPUT_CRITICAL = 10.0;

[[nodiscard]] inline double trapezoid(double x, double a, double b, double c, double d) noexcept {
    if (x < a || x > d) {
        return 0.0;
    }
    if (b <= x && x <= c) {
        return 1.0;
    }
    if (x < b) {
        return (b > a) ? ((x - a) / (b - a)) : 1.0;
    }
    return (d > c) ? ((d - x) / (d - c)) : 1.0;
}

struct MembershipTerms {
    double ideal;
    double acceptable;
    double deviated;
    double critical;
};

[[nodiscard]] inline MembershipTerms compute_memberships(double deviation) noexcept {
    const double d = std::max(0.0, deviation);
    return MembershipTerms{
        .ideal = trapezoid(d, -1.0, 0.0, 0.0, 0.25),
        .acceptable = trapezoid(d, 0.0, 0.25, 0.4, 0.7),
        .deviated = trapezoid(d, 0.4, 0.7, 0.9, 1.2),
        .critical = trapezoid(d, 0.9, 1.2, 1e9, 1e9)
    };
}

[[nodiscard]] inline double parameter_health(double deviation) noexcept {
    const auto m = compute_memberships(deviation);
    const double total = m.ideal + m.acceptable + m.deviated + m.critical;
    if (total <= 1e-12) {
        return OUTPUT_CRITICAL;
    }
    const double weighted_sum = OUTPUT_IDEAL * m.ideal +
                               OUTPUT_ACCEPTABLE * m.acceptable +
                               OUTPUT_DEVIATED * m.deviated +
                               OUTPUT_CRITICAL * m.critical;
    return weighted_sum / total;
}

[[nodiscard]] inline double get_weight(std::string_view parameter) noexcept {
    if (parameter == "ph") return 1.3;
    if (parameter == "temperature") return 1.2;
    if (parameter == "soilMoisture" || parameter == "soil_moisture") return 1.2;
    if (parameter == "tds") return 1.1;
    if (parameter == "humidity") return 0.9;
    if (parameter == "brightness") return 0.8;
    return 1.0;
}

// Redondeo bancario (round to nearest even) para paridad exacta con Python
[[nodiscard]] inline double round_half_even(double x, int decimals = 1) noexcept {
    const double factor = std::pow(10.0, decimals);
    const double val = x * factor;
    const double floor_val = std::floor(val);
    const double diff = val - floor_val;

    double rounded = floor_val;
    if (diff > 0.5) {
        rounded = floor_val + 1.0;
    } else if (diff == 0.5) {
        if (std::fmod(floor_val, 2.0) != 0.0) {
            rounded = floor_val + 1.0;
        }
    }
    return rounded / factor;
}

inline SmartPotHealthResult compute_health_index(
    const char* const* param_names,
    const double* deviations,
    int count
) noexcept {
    SmartPotHealthResult res{};
    if (count <= 0 || !param_names || !deviations) {
        res.index = 0.0;
        std::strncpy(res.level, "UNKNOWN", sizeof(res.level) - 1);
        std::strncpy(res.label, "Sin datos", sizeof(res.label) - 1);
        res.param_count = 0;
        return res;
    }

    double weighted_scores_sum = 0.0;
    double weights_sum = 0.0;
    double worst_critical = 0.0;

    res.param_count = std::min(count, 8);

    for (int i = 0; i < res.param_count; ++i) {
        const double dev = deviations[i];
        const double score = parameter_health(dev);
        const double weight = get_weight(param_names[i]);

        weighted_scores_sum += score * weight;
        weights_sum += weight;

        const auto m = compute_memberships(dev);
        if (m.critical > worst_critical) {
            worst_critical = m.critical;
        }

        std::strncpy(res.param_names[i], param_names[i], sizeof(res.param_names[i]) - 1);
        res.param_names[i][sizeof(res.param_names[i]) - 1] = '\0';
        res.param_scores[i] = round_half_even(score, 1);
    }

    const double weighted = (weights_sum > 0.0) ? (weighted_scores_sum / weights_sum) : 0.0;
    const double cap = 100.0 - 60.0 * worst_critical;
    const double final_raw = std::min(weighted, cap);
    res.index = round_half_even(final_raw, 1);

    if (res.index >= 85.0) {
        std::strncpy(res.level, "EXCELLENT", sizeof(res.level) - 1);
        std::strncpy(res.label, "Excelente", sizeof(res.label) - 1);
    } else if (res.index >= 70.0) {
        std::strncpy(res.level, "GOOD", sizeof(res.level) - 1);
        std::strncpy(res.label, "Saludable", sizeof(res.label) - 1);
    } else if (res.index >= 50.0) {
        std::strncpy(res.level, "FAIR", sizeof(res.level) - 1);
        std::strncpy(res.label, "Aceptable", sizeof(res.label) - 1);
    } else if (res.index >= 30.0) {
        std::strncpy(res.level, "POOR", sizeof(res.level) - 1);
        std::strncpy(res.label, "En riesgo", sizeof(res.label) - 1);
    } else {
        std::strncpy(res.level, "CRITICAL", sizeof(res.level) - 1);
        std::strncpy(res.label, "Crítico", sizeof(res.label) - 1);
    }

    res.level[sizeof(res.level) - 1] = '\0';
    res.label[sizeof(res.label) - 1] = '\0';
    return res;
}

} // namespace smartpot::fuzzy
