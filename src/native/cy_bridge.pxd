from libc.stdint cimport int32_t, uint8_t

cdef extern from "smartpot/types.hpp":
    cdef struct SmartPotTheilSenResult:
        double slope
        double intercept
        double agreement

    cdef struct SmartPotHealthResult:
        double index
        char level[16]
        char label[32]
        int32_t param_count
        char param_names[8][32]
        double param_scores[8]

    cdef struct SmartPotParamRange:
        char parameter[32]
        double min_val
        double max_val
        char unit[16]
        char label[32]

    cdef struct SmartPotForecastResult:
        char parameter[32]
        double current
        double slope_per_hour
        double expected_in_3h
        char trend[16]
        int32_t has_hours_to_limit
        double hours_to_limit
        char limit[8]
        double confidence
        char message[256]

    cdef struct SmartPotPhysicalBound:
        double min_val
        double max_val

    cdef struct SmartPotQualityReport:
        int32_t rows
        int32_t cols
        double completeness
        double validity
        double outliers
        double score
        int32_t keep_count

cdef extern from "smartpot/capi.h":
    int smartpot_theil_sen(
        const double* hours,
        const double* values,
        int count,
        SmartPotTheilSenResult* out_result
    ) noexcept nogil

    int smartpot_fuzzy_health_index(
        const char* const* param_names,
        const double* deviations,
        int count,
        SmartPotHealthResult* out_result
    ) noexcept nogil

    int smartpot_forecast_describe(
        const SmartPotParamRange* range,
        double current,
        double slope,
        double intercept,
        double agreement,
        SmartPotForecastResult* out_result
    ) noexcept nogil

    int smartpot_quality_assess(
        const double* matrix,
        int rows,
        int cols,
        const SmartPotPhysicalBound* bounds,
        SmartPotQualityReport* out_report,
        uint8_t* out_keep_mask
    ) noexcept nogil

    int smartpot_features_normalize(
        const double* values,
        const SmartPotParamRange* ranges,
        int count,
        double* out_normalized
    ) noexcept nogil

    int smartpot_features_circular_hour(
        double hour,
        double* out_sin,
        double* out_cos
    ) noexcept nogil

    const char* smartpot_engine_version() noexcept nogil
    const char* smartpot_engine_build_info() noexcept nogil
