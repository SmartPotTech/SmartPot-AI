#pragma once

#include "types.hpp"

#ifdef __cplusplus
extern "C" {
#endif

SMARTPOT_API int smartpot_theil_sen(
    const double* hours,
    const double* values,
    int count,
    SmartPotTheilSenResult* out_result
);

SMARTPOT_API int smartpot_fuzzy_health_index(
    const char* const* param_names,
    const double* deviations,
    int count,
    SmartPotHealthResult* out_result
);

SMARTPOT_API int smartpot_forecast_describe(
    const SmartPotParamRange* range,
    double current,
    double slope,
    double intercept,
    double agreement,
    SmartPotForecastResult* out_result
);

SMARTPOT_API int smartpot_quality_assess(
    const double* matrix,
    int rows,
    int cols,
    const SmartPotPhysicalBound* bounds,
    SmartPotQualityReport* out_report,
    uint8_t* out_keep_mask
);

SMARTPOT_API int smartpot_features_normalize(
    const double* values,
    const SmartPotParamRange* ranges,
    int count,
    double* out_normalized
);

SMARTPOT_API int smartpot_features_circular_hour(
    double hour,
    double* out_sin,
    double* out_cos
);

SMARTPOT_API const char* smartpot_engine_version(void);

SMARTPOT_API const char* smartpot_engine_build_info(void);

#ifdef __cplusplus
}
#endif
