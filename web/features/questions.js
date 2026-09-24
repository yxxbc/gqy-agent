import { apiRequest } from "../core/api.js";
import { MAX_CUSTOM_ANSWER_CHARS } from "../core/constants.js";
import { makeIconSlot } from "../core/icons.js";
import { showToast } from "../core/toast.js";
import { countCharacters, focusComposerIfDesktop, updateControlState } from "./composer/input.js";
import { liveViewed } from "./conversation/chrome.js";
import { contentAdded, updateJumpButtonOffset } from "./conversation/scroll.js";
import { breakLiveText, clearTypingIndicator, ensureLiveArticle } from "./live/state.js";
import { finalizeLiveReasoning } from "./live/stream.js";
import { loadSessionView } from "./sessions/view.js";
import { elements } from "../state/elements.js";
import { state } from "../state/store.js";

export function questionHasAnswer(questionState, index = questionState.pageIndex) {
  const control = questionState.controls[index];
  if (!control) return false;
  return control.options.some((option) => option.input.checked)
    || Boolean(control.custom?.toggle.checked && control.custom.textarea.value.trim());
}

export function updateQuestionNavigation(questionState) {
  if (!questionState?.questions?.length) return;
  const lastIndex = questionState.questions.length - 1;
  const atLastPage = questionState.pageIndex === lastIndex;
  const answered = questionHasAnswer(questionState);
  const canInteract = questionState.pending && !questionState.submitting && !questionState.closing;

  questionState.previous.disabled = !canInteract || questionState.pageIndex === 0;
  questionState.next.hidden = atLastPage;
  questionState.next.disabled = !canInteract || !answered;
  questionState.next.classList.toggle("is-ready", canInteract && answered && !atLastPage);
  questionState.submit.hidden = !atLastPage;
  questionState.submit.disabled = !canInteract || !answered;
  questionState.submit.classList.toggle("is-ready", canInteract && answered && atLastPage);
  questionState.close.disabled = !canInteract;

  questionState.controls.forEach((control, index) => {
    const custom = control.custom;
    if (!custom?.next) return;
    const customAnswered = Boolean(custom.toggle.checked && custom.textarea.value.trim());
    const show = canInteract && customAnswered;
    custom.next.hidden = !show;
    custom.next.disabled = !show;
    custom.next.classList.toggle("is-ready", show);
    custom.next.replaceChildren(makeIconSlot(index === lastIndex ? "check" : "chevron-right"));
    custom.next.title = index === lastIndex ? "提交回答" : "下一题";
    custom.next.setAttribute("aria-label", custom.next.title);
  });
}

export function updateQuestionOptionClasses(questionState) {
  for (const control of questionState.controls) {
    for (const option of control.options) option.label.classList.toggle("selected", option.input.checked);
    if (control.custom) control.custom.wrapper.classList.toggle("selected", control.custom.toggle.checked);
  }
  updateQuestionNavigation(questionState);
}

export function updateQuestionDock() {
  elements.questionDock.hidden = elements.questionDock.childElementCount === 0;
  elements.composerDock.classList.toggle("has-pending-question", !elements.questionDock.hidden);
  window.requestAnimationFrame(updateJumpButtonOffset);
}

export function clearQuestionDock() {
  elements.questionDock.replaceChildren();
  updateQuestionDock();
}

export function moveQuestionToTimeline(questionState) {
  if (questionState.card.parentElement !== elements.questionDock) return;
  if (questionState.timelineParent?.isConnected) questionState.timelineParent.appendChild(questionState.card);
  else questionState.card.remove();
  updateQuestionDock();
}

export function removeQuestionFromDock(questionState) {
  if (questionState.card.parentElement === elements.questionDock) questionState.card.remove();
  updateQuestionDock();
}

export function setQuestionPage(questionState, index, { focus = false } = {}) {
  if (!questionState?.pages?.length) return;
  if (questionState.autoAdvanceTimer) {
    window.clearTimeout(questionState.autoAdvanceTimer);
    questionState.autoAdvanceTimer = null;
  }
  const lastIndex = questionState.pages.length - 1;
  const nextIndex = Math.max(0, Math.min(lastIndex, Number(index) || 0));
  questionState.pageIndex = nextIndex;
  questionState.pages.forEach((page, pageIndex) => {
    page.hidden = pageIndex !== nextIndex;
  });
  const question = questionState.questions[nextIndex] || {};
  questionState.prompt.textContent = String(question.question || question.header || `问题 ${nextIndex + 1}`);
  questionState.position.textContent = `${nextIndex + 1} of ${questionState.pages.length}`;
  updateQuestionNavigation(questionState);
  elements.questionDock.scrollTop = 0;
  window.requestAnimationFrame(() => {
    updateJumpButtonOffset();
    if (focus) questionState.pages[nextIndex].querySelector("input:not(:disabled), textarea:not(:disabled)")?.focus();
  });
}

export function advanceQuestion(questionState) {
  if (!questionState?.pending || questionState.submitting || !questionHasAnswer(questionState)) return;
  if (questionState.pageIndex >= questionState.pages.length - 1) {
    submitQuestion(questionState);
    return;
  }
  setQuestionPage(questionState, questionState.pageIndex + 1, { focus: true });
}

export function selectedQuestionAnswers(questionState) {
  const answers = [];
  for (let index = 0; index < questionState.controls.length; index += 1) {
    const control = questionState.controls[index];
    const selected = control.options.filter((option) => option.input.checked).map((option) => option.value);
    if (control.custom?.toggle.checked) {
      const custom = control.custom.textarea.value.trim();
      if (!custom) throw new Error(`请填写第 ${index + 1} 项的自定义回答`);
      if (countCharacters(custom) > MAX_CUSTOM_ANSWER_CHARS) throw new Error(`第 ${index + 1} 项的自定义回答不能超过 4,000 个字符`);
      if (/[\u0000-\u001f\u007f-\u009f]/.test(custom)) throw new Error(`第 ${index + 1} 项的自定义回答不能包含控制字符或换行`);
      if (selected.includes(custom)) throw new Error(`第 ${index + 1} 项包含重复回答`);
      selected.push(custom);
    }
    if (selected.length === 0) throw new Error(`请回答第 ${index + 1} 项`);
    if (!control.multiple && selected.length !== 1) throw new Error(`第 ${index + 1} 项只能选择一个回答`);
    answers.push(selected);
  }
  return answers;
}

export function setQuestionControlsDisabled(questionState, disabled) {
  questionState.form.querySelectorAll("input, textarea, button").forEach((control) => {
    control.disabled = disabled;
  });
}

export function renderQuestionAnswerSummary(questionState, answers) {
  questionState.summary.replaceChildren();
  const normalized = Array.isArray(answers) ? answers : [];
  questionState.questions.forEach((question, index) => {
    const row = document.createElement("div");
    const term = document.createElement("dt");
    term.textContent = String(question?.question || question?.header || `问题 ${index + 1}`);
    const value = document.createElement("dd");
    value.textContent = (Array.isArray(normalized[index]) ? normalized[index] : []).map(String).join("、") || "未记录";
    row.append(term, value);
    questionState.summary.appendChild(row);
  });
  questionState.summary.hidden = false;
}

export function markQuestionAnswered(questionState, answers) {
  if (!questionState || !questionState.pending) return;
  if (questionState.autoAdvanceTimer) window.clearTimeout(questionState.autoAdvanceTimer);
  questionState.autoAdvanceTimer = null;
  questionState.pending = false;
  questionState.submitting = false;
  questionState.closing = false;
  questionState.restoreFocusOnClose = false;
  questionState.answers = answers;
  questionState.card.classList.remove("is-error");
  questionState.card.classList.add("is-answered");
  questionState.card.removeAttribute("aria-busy");
  questionState.header.hidden = false;
  questionState.card.removeAttribute("aria-label");
  questionState.card.setAttribute("aria-labelledby", questionState.titleId);
  questionState.status.textContent = "已回答";
  // 去掉那个大对钩(#143/#161):和落库回看的已回答卡一致,「已回答」二字已够表达状态。
  questionState.icon.replaceChildren();
  questionState.icon.hidden = true;
  questionState.error.hidden = true;
  setQuestionControlsDisabled(questionState, true);
  renderQuestionAnswerSummary(questionState, answers);
  moveQuestionToTimeline(questionState);
  updateControlState();
  contentAdded(questionState.card);
}

export function markQuestionClosed(questionState) {
  if (!questionState?.pending) return;
  const restoreFocus = questionState.restoreFocusOnClose || questionState.card.contains(document.activeElement);
  questionState.restoreFocusOnClose = false;
  if (questionState.autoAdvanceTimer) window.clearTimeout(questionState.autoAdvanceTimer);
  questionState.autoAdvanceTimer = null;
  questionState.pending = false;
  questionState.submitting = false;
  questionState.closing = false;
  questionState.card.removeAttribute("aria-busy");
  setQuestionControlsDisabled(questionState, true);
  removeQuestionFromDock(questionState);
  updateControlState();
  showToast("回答界面已关闭");
  if (restoreFocus) window.requestAnimationFrame(focusComposerIfDesktop);
  contentAdded(questionState.card);
}

export async function closeQuestion(questionState) {
  if (!questionState?.pending || questionState.submitting || questionState.closing) return;
  questionState.restoreFocusOnClose = questionState.card.contains(document.activeElement);
  questionState.closing = true;
  questionState.error.hidden = true;
  questionState.card.classList.remove("is-error");
  questionState.card.setAttribute("aria-busy", "true");
  questionState.close.replaceChildren(makeIconSlot("loader-circle", "is-spinning"));
  questionState.close.title = "正在关闭";
  questionState.close.setAttribute("aria-label", "正在关闭");
  setQuestionControlsDisabled(questionState, true);
  try {
    await apiRequest(`/api/questions/${encodeURIComponent(questionState.id)}`, { method: "DELETE" });
    if (questionState.pending) markQuestionClosed(questionState);
  } catch (error) {
    if (!questionState.pending) return;
    const restoreFocus = questionState.restoreFocusOnClose;
    questionState.restoreFocusOnClose = false;
    questionState.closing = false;
    questionState.card.removeAttribute("aria-busy");
    questionState.error.textContent = error.message || "回答界面关闭失败";
    questionState.error.hidden = false;
    questionState.card.classList.add("is-error");
    questionState.close.replaceChildren(makeIconSlot("x"));
    questionState.close.title = "关闭回答";
    questionState.close.setAttribute("aria-label", "关闭回答");
    setQuestionControlsDisabled(questionState, false);
    updateQuestionNavigation(questionState);
    showToast(error.message || "回答界面关闭失败", "error");
    if (restoreFocus) window.requestAnimationFrame(() => questionState.close.focus());
    if ((error.status === 404 || error.status === 409) && state.viewSessionId) {
      window.setTimeout(() => loadSessionView(state.viewSessionId, { quiet: true }), 300);
    }
  }
}

export async function submitQuestion(questionState) {
  if (!questionState.pending || questionState.submitting) return;
  let answers;
  try {
    answers = selectedQuestionAnswers(questionState);
  } catch (error) {
    const page = String(error.message || "").match(/第 (\d+) 项/);
    if (page) setQuestionPage(questionState, Number(page[1]) - 1);
    questionState.error.textContent = error.message;
    questionState.error.hidden = false;
    questionState.card.classList.add("is-error");
    return;
  }
  questionState.submitting = true;
  questionState.error.hidden = true;
  questionState.card.classList.remove("is-error");
  questionState.card.setAttribute("aria-busy", "true");
  questionState.submit.replaceChildren(makeIconSlot("loader-circle", "is-spinning"));
  questionState.submit.title = "提交中";
  questionState.submit.setAttribute("aria-label", "提交中");
  setQuestionControlsDisabled(questionState, true);
  try {
    await apiRequest(`/api/questions/${encodeURIComponent(questionState.id)}/answer`, {
      method: "POST",
      body: JSON.stringify({ answers })
    });
    if (questionState.pending) markQuestionAnswered(questionState, answers);
  } catch (error) {
    if (!questionState.pending) return;
    questionState.submitting = false;
    questionState.card.removeAttribute("aria-busy");
    questionState.error.textContent = error.message || "回答提交失败";
    questionState.error.hidden = false;
    questionState.card.classList.add("is-error");
    questionState.submit.replaceChildren(makeIconSlot("check"));
    questionState.submit.title = "提交回答";
    questionState.submit.setAttribute("aria-label", "提交回答");
    setQuestionControlsDisabled(questionState, false);
    updateQuestionNavigation(questionState);
    showToast(error.message || "回答提交失败", "error");
    if ((error.status === 404 || error.status === 409) && state.viewSessionId) {
      window.setTimeout(() => loadSessionView(state.viewSessionId, { quiet: true }), 300);
    }
  }
}

export function createQuestion(live, data) {
  clearTypingIndicator(live, { waitingOnly: true });
  const questionId = String(data?.question_id || "");
  if (!questionId) return null;
  if (live.questions.has(questionId)) return live.questions.get(questionId);
  ensureLiveArticle(live);
  breakLiveText(live);
  finalizeLiveReasoning(live);
  live.contextOperation = null;
  const questions = Array.isArray(data?.questions) ? data.questions : [];
  const card = document.createElement("section");
  card.className = "question-card";
  card.dataset.questionId = questionId;
  const titleId = `live-question-title-${live.questions.size + 1}`;
  card.setAttribute("aria-label", "待回答问题");
  const header = document.createElement("header");
  header.hidden = true;
  const icon = document.createElement("span");
  icon.className = "question-icon";
  icon.appendChild(makeIconSlot("circle-help"));
  const headerCopy = document.createElement("div");
  const status = document.createElement("small");
  status.textContent = "等待回答";
  const title = document.createElement("strong");
  title.id = titleId;
  title.textContent = questions.length === 1 ? String(questions[0]?.header || "补充确认") : `${questions.length} 项补充确认`;
  headerCopy.append(status, title);
  header.append(icon, headerCopy);
  const form = document.createElement("form");
  form.className = "question-form";
  const heading = document.createElement("div");
  heading.className = "question-heading";
  const prompt = document.createElement("p");
  prompt.className = "question-prompt";
  prompt.id = `question-${questionId}-prompt`;
  prompt.setAttribute("aria-live", "polite");
  prompt.setAttribute("aria-atomic", "true");
  prompt.textContent = String(questions[0]?.question || questions[0]?.header || "问题 1");
  const navigation = document.createElement("div");
  navigation.className = "question-navigation";
  navigation.setAttribute("role", "group");
  navigation.setAttribute("aria-label", "问题导航");
  const previous = document.createElement("button");
  previous.type = "button";
  previous.className = "question-page-button is-previous";
  previous.title = "上一题";
  previous.setAttribute("aria-label", "上一题");
  previous.appendChild(makeIconSlot("chevron-right"));
  const position = document.createElement("span");
  position.className = "question-position";
  position.textContent = `1 of ${questions.length}`;
  position.setAttribute("aria-live", "polite");
  const next = document.createElement("button");
  next.type = "button";
  next.className = "question-page-button";
  next.title = "下一题";
  next.setAttribute("aria-label", "下一题");
  next.appendChild(makeIconSlot("chevron-right"));
  const submit = document.createElement("button");
  submit.className = "question-page-button question-submit";
  submit.type = "submit";
  submit.title = "提交回答";
  submit.setAttribute("aria-label", "提交回答");
  submit.hidden = true;
  submit.appendChild(makeIconSlot("check"));
  const close = document.createElement("button");
  close.type = "button";
  close.className = "question-page-button question-close-button";
  close.title = "关闭回答";
  close.setAttribute("aria-label", "关闭回答");
  close.appendChild(makeIconSlot("x"));
  navigation.append(previous, position, next, submit, close);
  heading.append(prompt, navigation);
  form.appendChild(heading);
  const controls = [];
  const pages = [];
  questions.forEach((question, questionIndex) => {
    const fieldset = document.createElement("fieldset");
    fieldset.className = "question-fieldset";
    fieldset.id = `question-${questionId}-page-${questionIndex + 1}`;
    fieldset.setAttribute("aria-labelledby", prompt.id);
    fieldset.hidden = questionIndex !== 0;
    const legend = document.createElement("legend");
    legend.className = "question-legend";
    legend.setAttribute("aria-hidden", "true");
    legend.textContent = String(question?.question || question?.header || `问题 ${questionIndex + 1}`);
    fieldset.appendChild(legend);
    const optionList = document.createElement("div");
    optionList.className = "question-options";
    const multiple = Boolean(question?.multiple);
    const inputType = multiple ? "checkbox" : "radio";
    const inputName = `question-${questionId}-${questionIndex}`;
    const options = [];
    for (const option of Array.isArray(question?.options) ? question.options : []) {
      const label = document.createElement("label");
      label.className = "question-option";
      const input = document.createElement("input");
      input.type = inputType;
      input.name = inputName;
      input.value = String(option?.label || "");
      input.dataset.questionIndex = String(questionIndex);
      const optionCopy = document.createElement("span");
      optionCopy.className = "question-option-copy";
      const optionLabel = document.createElement("strong");
      optionLabel.textContent = String(option?.label || "");
      optionCopy.appendChild(optionLabel);
      if (String(option?.description || "")) {
        const description = document.createElement("small");
        description.textContent = String(option.description);
        optionCopy.appendChild(description);
      }
      label.append(input, optionCopy);
      optionList.appendChild(label);
      options.push({ input, label, value: String(option?.label || "") });
    }
    fieldset.appendChild(optionList);
    let custom = null;
    if (question?.custom !== false) {
      const wrapper = document.createElement("div");
      wrapper.className = "custom-answer";
      const toggle = document.createElement("input");
      toggle.type = inputType;
      toggle.name = inputName;
      toggle.value = "__custom__";
      toggle.dataset.questionIndex = String(questionIndex);
      toggle.setAttribute("aria-label", `${question?.header || `问题 ${questionIndex + 1}`}使用自定义回答`);
      const textarea = document.createElement("textarea");
      textarea.rows = 1;
      textarea.placeholder = "自定义回答";
      textarea.setAttribute("aria-label", `${question?.header || `问题 ${questionIndex + 1}`}的自定义回答`);
      textarea.addEventListener("focus", () => {
        toggle.checked = true;
        updateQuestionOptionClasses(questionState);
      });
      textarea.addEventListener("input", () => {
        toggle.checked = Boolean(textarea.value.trim());
        updateQuestionOptionClasses(questionState);
      });
      let customNext = null;
      if (!multiple) {
        customNext = document.createElement("button");
        customNext.type = "button";
        customNext.className = "custom-answer-next";
        customNext.title = "下一题";
        customNext.setAttribute("aria-label", "下一题");
        customNext.hidden = true;
        customNext.appendChild(makeIconSlot("chevron-right"));
        customNext.addEventListener("click", () => advanceQuestion(questionState));
      }
      wrapper.append(toggle, textarea);
      if (customNext) wrapper.appendChild(customNext);
      fieldset.appendChild(wrapper);
      custom = { wrapper, toggle, textarea, next: customNext };
    }
    form.appendChild(fieldset);
    pages.push(fieldset);
    controls.push({ multiple, options, custom });
  });
  const error = document.createElement("p");
  error.className = "question-error";
  error.setAttribute("role", "alert");
  error.hidden = true;
  form.appendChild(error);
  const summary = document.createElement("dl");
  summary.className = "question-answer-summary";
  summary.hidden = true;
  card.append(header, form, summary);
  const questionState = {
    id: questionId,
    runId: live.runId,
    questions,
    card,
    header,
    titleId,
    form,
    controls,
    pages,
    pageIndex: 0,
    prompt,
    position,
    previous,
    next,
    icon,
    status,
    submit,
    close,
    error,
    summary,
    timelineParent: live.blocks,
    pending: true,
    submitting: false,
    closing: false,
    restoreFocusOnClose: false,
    autoAdvanceTimer: null,
    answers: null
  };
  form.querySelectorAll("input").forEach((input) => input.addEventListener("change", () => {
    updateQuestionOptionClasses(questionState);
    const questionIndex = Number(input.dataset.questionIndex);
    const control = questionState.controls[questionIndex];
    if (!input.checked || input.value === "__custom__" || control?.multiple || questionIndex >= questionState.pages.length - 1) return;
    window.clearTimeout(questionState.autoAdvanceTimer);
    questionState.autoAdvanceTimer = window.setTimeout(() => {
      questionState.autoAdvanceTimer = null;
      if (questionState.pageIndex !== questionIndex || !input.checked) return;
      advanceQuestion(questionState);
    }, 120);
  }));
  previous.addEventListener("click", () => setQuestionPage(questionState, questionState.pageIndex - 1, { focus: true }));
  next.addEventListener("click", () => advanceQuestion(questionState));
  close.addEventListener("click", () => closeQuestion(questionState));
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    submitQuestion(questionState);
  });
  live.questions.set(questionId, questionState);
  // 离屏 live 的问题卡先游离,切回时 reattachLiveArticles 归位。
  if (liveViewed(live)) elements.questionDock.appendChild(card);
  updateQuestionDock();
  setQuestionPage(questionState, 0);
  updateQuestionOptionClasses(questionState);
  updateControlState();
  contentAdded(live);
  return questionState;
}

export function endPendingQuestions(live, message) {
  for (const question of live.questions.values()) {
    if (!question.pending) continue;
    if (question.autoAdvanceTimer) window.clearTimeout(question.autoAdvanceTimer);
    question.autoAdvanceTimer = null;
    question.pending = false;
    question.submitting = false;
    question.closing = false;
    question.restoreFocusOnClose = false;
    question.card.removeAttribute("aria-busy");
    question.card.classList.add("is-error");
    question.status.textContent = "本轮已结束";
    question.error.textContent = message;
    question.error.hidden = false;
    setQuestionControlsDisabled(question, true);
    removeQuestionFromDock(question);
  }
}
