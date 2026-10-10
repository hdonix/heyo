//! Answers in the making. The model's answer to a conversation's last
//! question is written here, not in the view that shows it: the palette may
//! close while the model is still answering, and the answer is in the
//! conversation all the same when it is opened again.

import type { Turn } from "../ai";
import { answered, turnsOf, type Conversation, type Conversations } from "./conversations";

/// Asks the model; `onText` is told the answer so far as it grows.
export type Ask = (options: { prompt: string; history: Turn[]; onText(text: string): void; signal: AbortSignal }) => Promise<string>;

/// How an answer ended.
export interface Outcome {
  /// The conversation as it is kept now; nothing if it was deleted meanwhile.
  conversation?: Conversation;
  /// Why there is no (whole) answer, in words for the user.
  error?: string;
}

/// An answer being written.
export interface Answer {
  /// What there is of it so far.
  text: string;
  settled: Promise<Outcome>;
}

interface Job extends Answer {
  controller: AbortController;
  /// The conversation was deleted: nothing of the answer is to be kept.
  dropped: boolean;
}

export function answersFor(store: Conversations, ask: Ask, describe: (error: unknown) => string, now: () => number = Date.now) {
  const jobs = new Map<string, Job>();

  /// Adds the answer to the conversation as it is kept by now (it may have
  /// been renamed since the question was asked).
  const keep = (asked: Conversation, text: string, partial: boolean) => {
    const next = answered(store.get(asked.id) ?? asked, text, now(), partial);
    store.save(next);
    return next;
  };

  return {
    /// The answer being written for a conversation, if one is.
    of(id: string): Answer | undefined {
      return jobs.get(id);
    },

    /// Has the conversation's last question answered, and the answer kept.
    /// One answer at a time for a conversation: a second call returns the
    /// one under way.
    start(conversation: Conversation): Answer {
      const running = jobs.get(conversation.id);
      if (running) return running;
      const { messages } = conversation;
      const question = messages.at(-1);
      if (question?.role !== "user") throw new Error("There is no question to answer");

      const controller = new AbortController();
      const job: Job = { text: "", controller, dropped: false, settled: Promise.resolve({}) };
      const request = { prompt: question.content, history: turnsOf(messages.slice(0, -1)), onText: (text: string) => void (job.text = text), signal: controller.signal };
      // Within a promise, so that failing to start is failing like any other.
      job.settled = Promise.resolve(request)
        .then(ask)
        .then((answer): Outcome => (job.dropped ? {} : { conversation: keep(conversation, answer.trim(), false) }))
        .catch((error): Outcome => {
          if (job.dropped) return {};
          // What had arrived is kept, and marked as cut short.
          const partial = job.text.trim();
          const kept = partial ? keep(conversation, partial, true) : (store.get(conversation.id) ?? conversation);
          // Stopped by the user: that is not a failure to report.
          return { conversation: kept, error: controller.signal.aborted ? undefined : describe(error) };
        })
        .finally(() => {
          if (jobs.get(conversation.id) === job) jobs.delete(conversation.id);
        });
      jobs.set(conversation.id, job);
      return job;
    },

    /// Stops the answer being written, keeping what there is of it.
    stop(id: string) {
      jobs.get(id)?.controller.abort();
    },

    /// Stops answers whose conversations are being deleted (all, without
    /// an id): nothing of them is written afterwards.
    drop(id?: string) {
      for (const [key, job] of jobs) {
        if (id !== undefined && key !== id) continue;
        job.dropped = true;
        job.controller.abort();
        jobs.delete(key);
      }
    },
  };
}

export type Answers = ReturnType<typeof answersFor>;
