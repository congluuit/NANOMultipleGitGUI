const NO_ASK_KEY = "nanomgg.aiReview.noAsk";

/** The user ticked "Don't ask again" before an AI review. */
export function reviewConsentRemembered(): boolean {
  try {
    return localStorage.getItem(NO_ASK_KEY) === "1";
  } catch {
    return false;
  }
}

export function rememberReviewConsent(on: boolean) {
  try {
    if (on) localStorage.setItem(NO_ASK_KEY, "1");
  } catch {
    // Private storage off: the dialog simply asks again next time.
  }
}

/** Settings: ask again before every review. */
export function forgetReviewConsent() {
  try {
    localStorage.removeItem(NO_ASK_KEY);
  } catch {
    // Nothing was stored.
  }
}
