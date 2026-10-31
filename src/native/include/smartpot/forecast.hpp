#pragma once

#include "types.hpp"
#include <vector>
#include <cmath>
#include <algorithm>
#include <cstring>
#include <cstdio>
#include <string>

namespace smartpot::forecast {

inline constexpr double MIN_SPAN_HOURS = 0.3;
inline constexpr double HORIZON_HOURS = 3.0;
inline constexpr double MAX_HOURS_TO_LIMIT = 24.0;
inline constexpr double STABLE_SHARE = 0.10;

[[nodiscard]] inline double compute_median(std::vector<double>& v) noexcept {
    if (v.empty()) {
        return 0.0;
    }
    const size_t n = v.size();
    const size_t mid = n / 2;

    std::nth_element(v.begin(), v.begin() + mid, v.end());
    double mid_val = v[mid];

    if (n % 2 == 1) {
        return mid_val;
    }

    auto max_it = std::max_element(v.begin(), v.begin() + mid);
    double prev_val = *max_it;
    return (prev_val + mid_val) * 0.5;
}

[[nodiscard]] inline int get_sign(double val) noexcept {
    if (val > 1e-12) return 1;
    if (val < -1e-12) return -1;
    return 0;
}

inline SmartPotTheilSenResult calculate_theil_sen(
    const double* hours,
    const double* values,
    size_t count
) noexcept {
    SmartPotTheilSenResult res{0.0, 0.0, 0.0};
    if (count < 2 || !hours || !values) {
        if (count == 1 && values) {
            res.intercept = values[0];
        }
        return res;
    }

    const size_t max_pairs = (count * (count - 1)) / 2;
    std::vector<double> slopes;
    slopes.reserve(max_pairs);

    for (size_t i = 0; i < count; ++i) {
        for (size_t j = i + 1; j < count; ++j) {
            const double dt = hours[j] - hours[i];
            if (dt > 1e-9) {
                slopes.push_back((values[j] - values[i]) / dt);
            }
        }
    }

    if (slopes.empty()) {
        std::vector<double> val_copy(values, values + count);
        res.slope = 0.0;
        res.intercept = compute_median(val_copy);
        res.agreement = 0.0;
        return res;
    }

    // 1. Pendiente mediana en O(M)
    res.slope = compute_median(slopes);

    // 2. Intercepto mediano en O(N)
    std::vector<double> residuals(count);
    for (size_t i = 0; i < count; ++i) {
        residuals[i] = values[i] - res.slope * hours[i];
    }
    res.intercept = compute_median(residuals);

    // 3. Concordancia (fracción de pendientes con el signo de la mediana)
    const int slope_sign = get_sign(res.slope);
    if (slope_sign == 0) {
        res.agreement = 1.0;
    } else {
        size_t matching = 0;
        for (double s : slopes) {
            if (get_sign(s) == slope_sign) {
                ++matching;
            }
        }
        res.agreement = static_cast<double>(matching) / static_cast<double>(slopes.size());
    }

    return res;
}

inline std::string fmt_value(double val) {
    char buf[32];
    std::snprintf(buf, sizeof(buf), "%.1f", val);
    std::string s(buf);
    while (s.find('.') != std::string::npos && (s.back() == '0' || s.back() == '.')) {
        bool dot = (s.back() == '.');
        s.pop_back();
        if (dot) break;
    }
    return s;
}

inline std::string fmt_hours(double hours) {
    if (hours < 1.0) {
        int minutes = static_cast<int>(std::round(hours * 60.0 / 5.0) * 5.0);
        if (minutes < 5) minutes = 5;
        return "unos " + std::to_string(minutes) + " minutos";
    }
    if (hours >= 1.5) {
        int h = static_cast<int>(std::round(hours));
        return "unas " + std::to_string(h) + " h";
    }
    return "alrededor de 1 h";
}

inline SmartPotForecastResult describe_forecast(
    const SmartPotParamRange& range,
    double current,
    double slope,
    double intercept,
    double agreement
) noexcept {
    SmartPotForecastResult res{};
    std::strncpy(res.parameter, range.parameter, sizeof(res.parameter) - 1);
    res.current = std::round(current * 100.0) / 100.0;
    res.slope_per_hour = std::round(slope * 1000.0) / 1000.0;
    res.expected_in_3h = std::round((intercept + slope * HORIZON_HOURS) * 100.0) / 100.0;
    res.confidence = std::round(agreement * 100.0) / 100.0;

    const double width = range.max_val - range.min_val;
    if (std::abs(slope) * HORIZON_HOURS >= STABLE_SHARE * width) {
        std::strncpy(res.trend, (slope > 0.0) ? "RISING" : "FALLING", sizeof(res.trend) - 1);
    } else {
        std::strncpy(res.trend, "STABLE", sizeof(res.trend) - 1);
    }

    const double level = intercept;
    res.has_hours_to_limit = 0;
    res.hours_to_limit = 0.0;
    res.limit[0] = '\0';

    if (std::strcmp(res.trend, "STABLE") != 0 && range.min_val <= level && level <= range.max_val) {
        double hours = (slope > 0.0) ? ((range.max_val - level) / slope) : ((level - range.min_val) / -slope);
        if (hours <= MAX_HOURS_TO_LIMIT) {
            res.has_hours_to_limit = 1;
            res.hours_to_limit = std::round(hours * 100.0) / 100.0;
            std::strncpy(res.limit, (slope > 0.0) ? "MAX" : "MIN", sizeof(res.limit) - 1);
        }
    }

    std::string label = range.label;
    if (!label.empty()) {
        label[0] = static_cast<char>(std::toupper(label[0]));
    }
    std::string per_hour = fmt_value(std::abs(slope)) + " " + range.unit + " por hora";

    std::string message;
    if (std::strcmp(res.trend, "STABLE") == 0) {
        message = label + " se mantiene estable.";
    } else if (res.has_hours_to_limit) {
        double bound = (std::strcmp(res.limit, "MAX") == 0) ? range.max_val : range.min_val;
        std::string verb = (slope > 0.0) ? "sube" : "baja";
        std::string bound_name = (std::strcmp(res.limit, "MAX") == 0) ? "máximo" : "mínimo";
        message = label + " " + verb + " " + per_hour + ": llegará al " + bound_name +
                  " (" + fmt_value(bound) + " " + range.unit + ") en " + fmt_hours(res.hours_to_limit) + ".";
    } else {
        std::string verb = (slope > 0.0) ? "sube" : "baja";
        message = label + " " + verb + " " + per_hour + ".";
    }

    std::strncpy(res.message, message.c_str(), sizeof(res.message) - 1);
    res.message[sizeof(res.message) - 1] = '\0';
    return res;
}

} // namespace smartpot::forecast
