// SPDX-License-Identifier: GPL-3.0-or-later
// Lazy loading shared by every place that fills thumbnails as they come into
// view: `nearView` says when an element is near enough, `loadQueue` runs the
// loads a few at a time. State lives in the closures, one pair per list.
"use strict";

// nearView(root, margin, fn, after) -> { watch(el), stop() }
// `fn(el)` runs once when `el` first comes within `margin` of `root` (null:
// the window); the element is then no longer watched. `after()`, when given,
// runs once after each batch of elements the browser reported together.
// `stop()` disconnects everything watched; `watch` works again after it.
function nearView(root, margin, fn, after) {
  const observer = new IntersectionObserver((entries) => {
    let any = false;
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      observer.unobserve(e.target);
      fn(e.target);
      any = true;
    }
    if (any) after?.();
  }, { root, rootMargin: margin });
  return { watch: (el) => observer.observe(el), stop: () => observer.disconnect() };
}

// loadQueue(limit, work, onIdle) -> { add(item), front(items), clear(), idle() }
// `work(item)` returns a promise (or nothing, to skip the item); at most
// `limit` run at once. `add` goes last, `front` (an array) goes first and
// keeps its order. `onIdle()` runs when nothing is waiting or running.
function loadQueue(limit, work, onIdle) {
  let items = [];
  let running = 0;
  const idle = () => running === 0 && items.length === 0;
  const pump = () => {
    while (running < limit && items.length) {
      const item = items.shift();
      let job;
      try {
        job = work(item);
      } catch {
        continue;
      }
      if (!job || typeof job.then !== "function") continue;
      running++;
      job.catch(() => {}).finally(() => {
        running--;
        pump();
      });
    }
    if (idle()) onIdle?.();
  };
  return {
    add(item) {
      items.push(item);
      pump();
    },
    front(list) {
      items = list.concat(items);
      pump();
    },
    clear() {
      items = [];
    },
    idle,
  };
}
