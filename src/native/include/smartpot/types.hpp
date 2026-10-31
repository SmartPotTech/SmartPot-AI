#pragma once

#include <cstdint>
#include <cstddef>

#ifdef _WIN32
  #ifdef SMARTPOT_EXPORTS
    #define SMARTPOT_API __declspec(dllexport)
  #else
    #define SMARTPOT_API __declspec(dllimport)
  #endif
#else
  #define SMARTPOT_API __attribute__((visibility("default")))
#endif

#pragma pack(push, 8)

extern "C" {

struct SmartPotTheilSenResult {
    double slope;
    double intercept;
    double agreement;
};

struct SmartPotHealthResult {
    double index;
    char level[16];
    char label[32];
    int32_t param_count;
    char param_names[8][32];
    double param_scores[8];
};

struct SmartPotParamRange {
    char parameter[32];
    double min_val;
    double max_val;
    char unit[16];
    char label[32];
};

struct SmartPotForecastResult {
    char parameter[32];
    double current;
    double slope_per_hour;
    double expected_in_3h;
    char trend[16];
    int32_t has_hours_to_limit;
    double hours_to_limit;
    char limit[8];
    double confidence;
    char message[256];
};

struct SmartPotPhysicalBound {
    double min_val;
    double max_val;
};

struct SmartPotQualityReport {
    int32_t rows;
    int32_t cols;
    double completeness;
    double validity;
    double outliers;
    double score;
    int32_t keep_count;
};

} // extern "C"

#pragma pack(pop)
