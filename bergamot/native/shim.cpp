#include "bergamot.h"

#include <cstdlib>
#include <cstring>
#include <exception>
#include <memory>
#include <mutex>
#include <sstream>
#include <string>
#include <vector>

#include "common/logging.h"
#include "translator/batch.h"
#include "translator/parser.h"
#include "translator/response_options.h"
#include "translator/service.h"
#include "translator/translation_model.h"

using namespace marian::bergamot;

struct bg_model {
  std::shared_ptr<TranslationModel> model;
};

namespace {

// Marian's per-batch arena, in MB. Firefox uses 128.
constexpr int kWorkspaceMb = 128;

// Every BlockingService registers the same named loggers globally, so the process has exactly one,
// shared by all models and used by one thread at a time. It is never destroyed: at exit its loggers
// would be dropped from a registry that may already be gone.
std::mutex engine_mutex;

BlockingService &engine() {
  static auto *service = [] {
    BlockingService::Config config;
    config.cacheSize = 0;
    return new BlockingService(config);
  }();
  return *service;
}

char *copy(const std::string &s) {
  auto *out = static_cast<char *>(std::malloc(s.size() + 1));
  if (out != nullptr) std::memcpy(out, s.c_str(), s.size() + 1);
  return out;
}

void fail(char **err, const char *message) {
  if (err != nullptr) *err = copy(message);
}

// A call that throws leaves the rest of its sentences queued on the model, holding callbacks into that
// call's stack frame. Drop them so the next call on the model doesn't run them.
void discard_pending(TranslationModel &model) {
  Batch batch;
  while (model.generateBatch(batch) > 0) {
  }
}

// A YAML double-quoted scalar, so paths with spaces, colons or quotes survive the config parser.
std::string quoted(const char *s) {
  std::string out = "\"";
  for (const char *c = s; *c != '\0'; ++c) {
    if (*c == '"' || *c == '\\') out += '\\';
    out += *c;
  }
  return out + "\"";
}

}  // namespace

extern "C" bg_model *bg_load(const char *model, const char *src_vocab, const char *trg_vocab,
                             const char *shortlist, char **err) {
  try {
    std::lock_guard<std::mutex> lock(engine_mutex);
    // Marian aborts the process on errors such as an unreadable model unless told to throw.
    marian::setThrowExceptionOnAbort(true);
    // Firefox's translations engine settings: greedy decoding with int8 GEMM, on the calling thread.
    std::ostringstream cfg;
    cfg << "models:\n  - " << quoted(model) << "\n"
        << "vocabs:\n  - " << quoted(src_vocab) << "\n  - " << quoted(trg_vocab) << "\n"
        << "shortlist:\n  - " << quoted(shortlist) << "\n  - false\n"
        << "beam-size: 1\nnormalize: 1.0\nword-penalty: 0\nmax-length-break: 128\n"
        << "mini-batch-words: 1024\nworkspace: " << kWorkspaceMb << "\n"
        << "max-length-factor: 2.0\nskip-cost: true\ncpu-threads: 0\nquiet: true\n"
        << "quiet-translation: true\ngemm-precision: int8shiftAlphaAll\nalignment: soft\n"
        << "ssplit-mode: paragraph\n";
    engine();
    auto translation_model = std::make_shared<TranslationModel>(parseOptionsFromString(cfg.str(), false));
    return new bg_model{std::move(translation_model)};
  } catch (const std::exception &e) {
    fail(err, e.what());
  } catch (...) {
    fail(err, "unknown error loading the model");
  }
  return nullptr;
}

extern "C" int bg_translate(bg_model *model, const char *const *texts, size_t n, char **out, char **err) {
  try {
    std::vector<std::string> sources(texts, texts + n);
    std::vector<ResponseOptions> options(n);
    std::vector<Response> responses;
    {
      std::lock_guard<std::mutex> lock(engine_mutex);
      try {
        responses = engine().translateMultiple(model->model, std::move(sources), options);
      } catch (...) {
        discard_pending(*model->model);
        throw;
      }
    }
    if (responses.size() != n) {
      fail(err, "the translator returned a different number of texts");
      return 1;
    }
    for (size_t i = 0; i < n; ++i) {
      out[i] = copy(responses[i].getTranslatedText());
      if (out[i] == nullptr) {
        for (size_t j = 0; j < i; ++j) std::free(out[j]);
        fail(err, "out of memory");
        return 1;
      }
    }
    return 0;
  } catch (const std::exception &e) {
    fail(err, e.what());
  } catch (...) {
    fail(err, "unknown error translating");
  }
  return 1;
}

extern "C" void bg_free_string(char *s) { std::free(s); }

extern "C" void bg_free(bg_model *model) {
  std::lock_guard<std::mutex> lock(engine_mutex);
  delete model;
}
