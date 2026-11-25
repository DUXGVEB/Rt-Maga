import type { FeedbackReportKind } from "../types";

export interface FeedbackPrefillDraft {
  kind: FeedbackReportKind;
  title: string;
  description: string;
  attachLogs: boolean;
}

class FeedbackDraftStore {
  pending = $state<FeedbackPrefillDraft | null>(null);

  setPrefill(draft: FeedbackPrefillDraft) {
    this.pending = draft;
  }

  consume(): FeedbackPrefillDraft | null {
    const val = this.pending;
    this.pending = null;
    return val;
  }
}

export const feedbackDraft = new FeedbackDraftStore();
