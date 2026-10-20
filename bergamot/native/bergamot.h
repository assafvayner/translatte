#ifndef BERGAMOT_SHIM_H
#define BERGAMOT_SHIM_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct bg_model bg_model;

/* Loads a model. On failure returns NULL and sets *err (free it with bg_free_string). */
bg_model *bg_load(const char *model, const char *src_vocab, const char *trg_vocab, const char *shortlist,
                  char **err);

/* Translates texts[0..n) as one batch on the calling thread, writing out[0..n) (free each with
 * bg_free_string). Returns 0 on success; otherwise sets *err and writes nothing to out. */
int bg_translate(bg_model *model, const char *const *texts, size_t n, char **out, char **err);

void bg_free_string(char *s);
void bg_free(bg_model *model);

#ifdef __cplusplus
}
#endif

#endif
