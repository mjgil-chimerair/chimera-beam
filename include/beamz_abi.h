#ifndef BEAMZ_ABI_H
#define BEAMZ_ABI_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Error codes returned by Zig kernels
typedef enum {
    BEAMZ_SUCCESS = 0,
    BEAMZ_INVALID_INPUT = 1,
    BEAMZ_INSUFFICIENT_BUFFER = 2,
    BEAMZ_MALFORMED_TERM = 3,
    BEAMZ_HEAP_EXHAUSTED = 4,
    BEAMZ_UNKNOWN_ERROR = 99
} BeamZErrorCode;

/// Generic result structure for Zig kernel operations
typedef struct {
    uint32_t code;
    size_t consumed;
    size_t produced;
} BeamZResult;

/// ETF scan result - includes version byte when found
typedef struct {
    uint32_t code;
    size_t consumed;
    uint8_t version;
} BeamZEtfScanResult;

/// ETF term size result
typedef struct {
    uint32_t code;
    size_t consumed;
    size_t produced;
} BeamZEtfTermSizeResult;

/// Term size result for individual term
typedef struct {
    uint32_t code;
    size_t size;
} BeamZTermSizeResult;

/// Scan ETF bytes for version header (131)
BeamZEtfScanResult beamz_etf_scan(const uint8_t* input_ptr, size_t input_len);

/// Calculate ETF encoded term size
BeamZResult beamz_etf_term_size(const uint8_t* input_ptr, size_t input_len);

/// Calculate term size for a tagged u64
size_t beamz_term_size(uint64_t term);

/// Copy cons cell from source to heap destination
void beamz_copy_cons(const uint8_t* src, uint8_t* dst, size_t len);

#ifdef __cplusplus
}
#endif

#endif // BEAMZ_ABI_H