#ifndef AXIOMME_MOBILE_FFI_H
#define AXIOMME_MOBILE_FFI_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct AxiommeRuntime AxiommeRuntime;

typedef int32_t AxiommeFfiCode;
enum {
  AXIOMME_FFI_CODE_OK = 0,
  AXIOMME_FFI_CODE_INVALID_ARGUMENT = 1,
  AXIOMME_FFI_CODE_RUNTIME_ERROR = 2
};

typedef struct AxiommeOwnedBytes {
  uint8_t *ptr;
  size_t len;
} AxiommeOwnedBytes;

typedef struct AxiommeFfiResult {
  AxiommeFfiCode code;
  AxiommeOwnedBytes payload;
} AxiommeFfiResult;

/**
 * Safety contract:
 * - root_dir: non-null UTF-8 C string
 * - out_runtime: non-null writable pointer
 * - runtime must be freed with axiomme_runtime_free
 */
AxiommeFfiResult axiomme_runtime_new(const char *root_dir, AxiommeRuntime **out_runtime);

/**
 * Safety contract:
 * - runtime must be a live pointer returned by axiomme_runtime_new
 */
AxiommeFfiResult axiomme_runtime_initialize(AxiommeRuntime *runtime);

/**
 * Safety contract:
 * - runtime must be a live pointer returned by axiomme_runtime_new
 * - returned payload must be freed with axiomme_owned_bytes_free
 */
AxiommeFfiResult axiomme_runtime_backend_status_json(AxiommeRuntime *runtime);

/**
 * Safety contract:
 * - runtime must be a live pointer returned by axiomme_runtime_new
 * - uri: non-null UTF-8 C string
 */
AxiommeFfiResult axiomme_runtime_mkdir(AxiommeRuntime *runtime, const char *uri);

/**
 * Safety contract:
 * - runtime must be a live pointer returned by axiomme_runtime_new
 * - uri: non-null UTF-8 C string
 */
AxiommeFfiResult axiomme_runtime_ls_json(AxiommeRuntime *runtime, const char *uri, bool recursive);

/**
 * Safety contract:
 * - runtime must be a live pointer returned by axiomme_runtime_new
 * - uri: non-null UTF-8 C string
 */
AxiommeFfiResult axiomme_runtime_load_markdown_json(AxiommeRuntime *runtime, const char *uri);

/**
 * Safety contract:
 * - runtime must be a live pointer returned by axiomme_runtime_new
 * - uri/content: non-null UTF-8 C strings
 * - expected_etag may be null
 */
AxiommeFfiResult axiomme_runtime_save_markdown_json(
    AxiommeRuntime *runtime,
    const char *uri,
    const char *content,
    const char *expected_etag
);

/**
 * Safety contract:
 * - runtime must be a live pointer returned by axiomme_runtime_new
 * - uri: non-null UTF-8 C string
 */
AxiommeFfiResult axiomme_runtime_rm(AxiommeRuntime *runtime, const char *uri, bool recursive);

/**
 * Safety contract:
 * - runtime must be null or a pointer from axiomme_runtime_new
 * - free exactly once
 */
void axiomme_runtime_free(AxiommeRuntime *runtime);

/**
 * Safety contract:
 * - bytes must come from axiomme runtime FFI calls
 * - free exactly once
 */
void axiomme_owned_bytes_free(AxiommeOwnedBytes bytes);

#ifdef __cplusplus
}
#endif

#endif
