// SPDX-License-Identifier: GPL-3.0-or-later
// Learn (F1): short lessons from lessons.json. A card shows one step at a
// time and points at the control it is about; a step with a goal moves on
// by itself when the photo's settings meet it (read after every edit, in
// learnStateChanged), so the lesson follows what you do.
"use strict";

const learn = { lessons: null, lesson: null, step: 0, start: {}, target: null };

async function loadLessons() {
  if (learn.lessons) return learn.lessons;
  const r = await fetch("lessons.json");
  learn.lessons = (await r.json()).lessons;
  return learn.lessons;
}

// The lesson picker.
async function learnFlow() {
  const lessons = await loadLessons();
  const dlg = $("learn-dialog");
  const list = $("learn-list");
  list.replaceChildren(...lessons.map((l) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "learn-item";
    const title = document.createElement("strong");
    title.textContent = l.title;
    const summary = document.createElement("span");
    summary.textContent = l.summary + (l.needs_photo && !ui.info ? " (Open a photo first.)" : "");
    b.append(title, summary);
    b.disabled = l.needs_photo && !ui.info;
    b.addEventListener("click", () => {
      dlg.close();
      startLesson(l);
    });
    return b;
  }));
  dlg.showModal();
}

function startLesson(lesson) {
  learn.lesson = lesson;
  showStep(0);
}

function endLesson() {
  clearLessonTarget();
  learn.lesson = null;
  $("lesson-card").hidden = true;
}

// The element a step points at: a setting's row, or an element by id.
function lessonTarget(h) {
  if (!h) return null;
  if (h.element) return $(h.element);
  const c = h.setting && ui.controls.get(h.setting);
  if (!c) return null;
  const el = c.label ?? c.check;
  return el?.closest(".row, .knob, .bar, .number-row") ?? el ?? null;
}

function clearLessonTarget() {
  learn.target?.classList.remove("lesson-target");
  learn.target = null;
}

function showStep(i) {
  const l = learn.lesson;
  learn.step = i;
  // Where the settings were when the step began, for "changed" goals.
  learn.start = { ...ui.settings };
  const step = l.steps[i];
  clearLessonTarget();
  const target = lessonTarget(step.highlight);
  if (target) {
    // Open the panel it's in and bring it into view.
    const panel = target.closest("details");
    if (panel) panel.open = true;
    target.classList.add("lesson-target");
    target.scrollIntoView({ block: "nearest", behavior: "smooth" });
    learn.target = target;
  }
  $("lesson-title").textContent = `${l.title} · ${i + 1} of ${l.steps.length}`;
  $("lesson-text").textContent = step.text;
  $("lesson-goal").hidden = !step.goal;
  $("lesson-back").disabled = i === 0;
  $("lesson-next").textContent = i + 1 === l.steps.length ? "Finish" : step.goal ? "Skip" : "Next";
  $("lesson-card").hidden = false;
}

function goalMet(goal) {
  const now = ui.settings ?? {};
  const value = (n) => (n in now ? Number(now[n]) : null);
  if (goal.changed) return JSON.stringify(now[goal.changed]) !== JSON.stringify(learn.start[goal.changed]);
  if (goal.setting) {
    const v = value(goal.setting) ?? 0;
    if (goal.above !== undefined && !(v > goal.above)) return false;
    if (goal.below !== undefined && !(v < goal.below)) return false;
    return true;
  }
  if (goal.history) return (ui.state?.history ?? []).some((h) => String(h).includes(goal.history));
  return false;
}

// Called after every edit (setState).
function learnStateChanged() {
  if (!learn.lesson) return;
  const step = learn.lesson.steps[learn.step];
  if (step.goal && goalMet(step.goal)) {
    $("lesson-goal").hidden = true;
    $("lesson-text").textContent = `✓ ${step.text}`;
    // A moment to see the tick, then on.
    const at = learn.step;
    setTimeout(() => { if (learn.lesson && learn.step === at) nextStep(); }, 900);
  }
}

function nextStep() {
  if (!learn.lesson) return;
  if (learn.step + 1 >= learn.lesson.steps.length) return endLesson();
  showStep(learn.step + 1);
}

function initLearn() {
  $("lesson-next").addEventListener("click", nextStep);
  $("lesson-back").addEventListener("click", () => learn.lesson && learn.step > 0 && showStep(learn.step - 1));
  $("lesson-close").addEventListener("click", endLesson);
  $("btn-learn-empty").addEventListener("click", () => learnFlow().catch((e) => toast(String(e), true)));
}
