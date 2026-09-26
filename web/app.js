import { apiRequest, onApiUnauthorized } from "./core/api.js";
import { DEFAULT_BOARD_SUBTITLE, DEFAULT_BOARD_TITLE, DEFAULT_STARTER_PROMPTS, defaultComposerPlaceholder } from "./core/constants.js";
import { formatFileSize, formatTokens } from "./core/format.js";
import { makeIconSlot, renderIconSlots } from "./core/icons.js";
import { safeStorageGet, safeStorageSet } from "./core/storage.js";
import { showToast } from "./core/toast.js";
import { UI_SCALE, layoutViewportWidth, start as start_core_ui_scale_js, visualPixelsToLayout } from "./core/ui-scale.js";
import { createInvite, saveAccount } from "./features/accounts.js";
import { probeMatugenTheme, setChatFontSize, setColorScheme, setProcCollapse, setReasoningExpanded, setTheme, setToolExpanded } from "./features/appearance.js";
import { closeArtifactResourceMenu, setArtifactWorkspaceOpen, syncArtifactLayout } from "./features/artifacts/model.js";
import { changeArtifactImageZoom, copySelectedArtifact, handleArtifactImageKey, setArtifactMode, toggleArtifactMaximized } from "./features/artifacts/workspace.js";
import { logout, showBlockedState, showRegisterForm, submitLogin, submitRegister, submitSetupAdmin } from "./features/auth.js";
import { loadBootstrap } from "./features/boot.js";
import { addComposerFiles, collectTransferFiles } from "./features/composer/attachments.js";
import { isTouchComposer, resizeComposer } from "./features/composer/input.js";
import { submitTurn } from "./features/composer/submit.js";
import { wireMicButton } from "./features/composer/voice.js";
import { bindConsoleEvents, consoleOpen, parseConsoleHash, parsePlatformView, setConsolePanel, setPlatformView } from "./features/console/panel.js";
import { start as start_features_console_usage_js } from "./features/console/usage.js";
import { conversationRunning } from "./features/conversation/chrome.js";
import { contentAdded, isAtBottom, isNearBottom, programmaticScrollSmooth, programmaticScrollTimer, scrollToBottom, suspendOutputFollowing, updateJumpButtonOffset } from "./features/conversation/scroll.js";
import { start as start_features_conversation_subagent_js } from "./features/conversation/subagent.js";
import { refreshSessionContext } from "./features/goal.js";
import { start as start_features_jobs_js } from "./features/jobs.js";
import { handleGlobalKeydown } from "./features/keyboard.js";
import { renderMarkdown } from "./features/markdown/render.js";
import { closeLevelMenu, closeModelMenu, openModelMenu, positionModelMenu, renderModelMenu } from "./features/model-menu/menu.js";
import { bindOobeEvents, openOobe } from "./features/oobe.js";
import { requestNewConversation, resetConversation } from "./features/session-mode.js";
import { closeSessionMenu, startBrailleTicker } from "./features/sessions/list.js";
import { loadSessionView } from "./features/sessions/view.js";
import { applyAdvancedConfig, clearProviderSecretChanges, configValue, loadConfigDraft, markConfigDirty, refreshProviderSecretStates, saveConfigDraft, setConfigValue, setSettingsView, updateAdvancedConfigEditor, updateSettingsControls } from "./features/settings/config.js";
import { closeSidebar, openSidebar, setSidebarCollapsed } from "./features/sidebar.js";
import { elements } from "./state/elements.js";
import { state } from "./state/store.js";

/// 只有本模块用的状态（从 state/store.js 分出来的私有分片）。
const appState = {
  composing: false
};

function bindEvents() {
  bindConsoleEvents();
  elements.mobileMenuButton.addEventListener("click", (event) => openSidebar(event.currentTarget));
  elements.sidebarClose.addEventListener("click", closeSidebar);
  elements.sidebarScrim.addEventListener("click", closeSidebar);
  elements.sidebarCollapseButton?.addEventListener("click", () => setSidebarCollapsed(true));
  elements.sidebarExpandButton?.addEventListener("click", () => setSidebarCollapsed(false));
  elements.artifactToggleButton.addEventListener("click", () => setArtifactWorkspaceOpen(!state.artifactOpen));
  elements.artifactCloseButton.addEventListener("click", () => setArtifactWorkspaceOpen(false));
  // 上下文圆环 → 分项弹窗(contextpanel.js)。压缩成功后的重拉与 /compact 命令同一条路。
  window.GqyContextPanel?.mount({
    trigger: elements.contextTrack,
    pop: document.getElementById("contextPop"),
    dock: elements.composerDock,
    apiRequest,
    formatTokens,
    toLayout: visualPixelsToLayout,
    uiScale: () => UI_SCALE,
    getSessionId: () => state.viewSessionId || state.currentSessionId,
    getContext: () => ({ tokens: state.context?.tokens, window: state.context?.window }),
    isRunning: () => conversationRunning(),
    onCompacted: async (sessionId) => {
      if (state.viewSessionId && state.viewSessionId !== state.currentSessionId) {
        await loadSessionView(state.viewSessionId, { quiet: true });
      } else {
        await loadBootstrap();
      }
      refreshSessionContext(sessionId);
    },
  });
  // 聊天正文选中文字的右键菜单(selectionmenu.js)。
  window.GqySelectionMenu?.mount({
    root: elements.chatScroll,
    composer: elements.composerInput,
    resizeComposer,
    apiRequest,
    renderMarkdown,
    getSessionId: () => state.viewSessionId || state.currentSessionId,
    toast: showToast,
  });
  elements.artifactPreviewButton.addEventListener("click", () => setArtifactMode("preview"));
  elements.artifactSourceButton.addEventListener("click", () => setArtifactMode("source"));
  elements.artifactImageZoomOutButton.addEventListener("click", () => changeArtifactImageZoom(-0.25));
  elements.artifactImageZoomInButton.addEventListener("click", () => changeArtifactImageZoom(0.25));
  document.addEventListener("keydown", handleArtifactImageKey);
  elements.artifactCopyButton.addEventListener("click", copySelectedArtifact);
  elements.artifactMaximizeButton.addEventListener("click", toggleArtifactMaximized);
  elements.artifactTitleButton.addEventListener("click", (event) => {
    event.stopPropagation();
    if (elements.artifactTitleButton.disabled) return;
    const opening = elements.artifactResourceMenu.hidden;
    elements.artifactResourceMenu.hidden = !opening;
    elements.artifactTitleButton.setAttribute("aria-expanded", String(opening));
  });
  elements.artifactResizeHandle.addEventListener("pointerdown", (event) => {
    if (layoutViewportWidth() <= 760 || state.artifactMaximized) return;
    event.preventDefault();
    elements.artifactResizeHandle.setPointerCapture(event.pointerId);
    const startX = event.clientX;
    const startWidth = elements.artifactWorkspace.offsetWidth;
    let resizeFrame = null;
    let nextRatio = state.artifactWidthRatio;
    const applyResize = () => {
      resizeFrame = null;
      state.artifactWidthRatio = nextRatio;
      syncArtifactLayout();
    };
    const move = (moveEvent) => {
      const viewportWidth = Math.max(320, layoutViewportWidth());
      const pointerDelta = visualPixelsToLayout(startX - moveEvent.clientX);
      const width = Math.min(viewportWidth - 20, Math.max(320, startWidth + pointerDelta));
      nextRatio = width / viewportWidth;
      if (!resizeFrame) resizeFrame = window.requestAnimationFrame(applyResize);
    };
    const finish = () => {
      if (resizeFrame) {
        window.cancelAnimationFrame(resizeFrame);
        applyResize();
      }
      safeStorageSet("gqy.web.artifactWidthRatio.v2", String(state.artifactWidthRatio));
      elements.artifactResizeHandle.removeEventListener("pointermove", move);
      elements.artifactResizeHandle.removeEventListener("pointerup", finish);
      elements.artifactResizeHandle.removeEventListener("pointercancel", finish);
    };
    elements.artifactResizeHandle.addEventListener("pointermove", move);
    elements.artifactResizeHandle.addEventListener("pointerup", finish);
    elements.artifactResizeHandle.addEventListener("pointercancel", finish);
  });
  elements.settingsNav.querySelectorAll("[data-settings-view]").forEach((button) => {
    button.addEventListener("click", () => setSettingsView(button.dataset.settingsView));
  });
  document.getElementById("openGroupsPanel")?.addEventListener("click", () => {
    state.platformView = { platform: "qq", tab: "groups" };
    setConsolePanel("platforms");
  });
  elements.consoleView.querySelectorAll("[data-platform]").forEach((button) => {
    button.addEventListener("click", () => setPlatformView(button.dataset.platform, ""));
  });
  elements.consoleView.querySelectorAll("[data-platform-tab]").forEach((button) => {
    button.addEventListener("click", () => setPlatformView(state.platformView.platform, button.dataset.platformTab));
  });
  window.GqySettings?.init({
    state,
    configValue,
    setConfigValue,
    markConfigDirty,
    updateAdvancedConfigEditor,
    updateSettingsControls,
    refreshProviderSecretStates,
    clearProviderSecretChanges,
    apiRequest,
    showToast,
    renderModelMenu,
    setSettingsView,
    DEFAULT_BOARD_TITLE,
    DEFAULT_BOARD_SUBTITLE,
    defaultComposerPlaceholder,
    DEFAULT_STARTER_PROMPTS
  });
  elements.reloadConfigButton.addEventListener("click", loadConfigDraft);
  elements.saveConfigButton.addEventListener("click", saveConfigDraft);
  elements.applyAdvancedConfigButton.addEventListener("click", applyAdvancedConfig);
  elements.sidebarThemeButton.addEventListener("click", () => setTheme(elements.body.dataset.theme === "graphite" ? "linen" : "graphite"));
  document.querySelectorAll("[data-theme-choice]").forEach((button) => button.addEventListener("click", () => setTheme(button.dataset.themeChoice)));
  document.querySelectorAll("[data-scheme-choice]").forEach((button) => button.addEventListener("click", () => setColorScheme(button.dataset.schemeChoice)));
  document.querySelectorAll("[data-chat-font]").forEach((button) => button.addEventListener("click", () => setChatFontSize(button.dataset.chatFont)));
  elements.reasoningExpandToggle?.addEventListener("click", () => setReasoningExpanded(!state.reasoningExpanded));
  elements.toolExpandToggle?.addEventListener("click", () => setToolExpanded(!state.toolExpanded));
  elements.procCollapseToggle?.addEventListener("click", () => setProcCollapse(!state.procCollapse));
  elements.modelButton.addEventListener("click", (event) => {
    event.stopPropagation();
    if (elements.modelMenu.hidden) openModelMenu();
    else closeModelMenu({ restoreFocus: true });
  });
  elements.modelMenu.addEventListener("keydown", (event) => {
    const items = Array.from(elements.modelMenu.querySelectorAll("button:not(:disabled)"));
    const index = items.indexOf(document.activeElement);
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const direction = event.key === "ArrowDown" ? 1 : -1;
      items[(index + direction + items.length) % items.length]?.focus();
    } else if (event.key === "Home" || event.key === "End") {
      // 过滤框里 Home/End 是光标跳首尾,不是列表跳首尾项。
      if (event.target instanceof HTMLInputElement) return;
      event.preventDefault();
      items[event.key === "Home" ? 0 : items.length - 1]?.focus();
    } else if (event.key === "Escape") {
      event.preventDefault();
      closeModelMenu({ restoreFocus: true });
    }
  });
  document.addEventListener("pointerdown", (event) => {
  });
  document.addEventListener("click", (event) => {
    if (!elements.modelLevelMenu.hidden && !event.target.closest("#modelLevelMenu")) {
      closeLevelMenu();
    }
    if (!elements.modelMenu.hidden
      && !event.target.closest("#modelMenuWrap")
      && !event.target.closest("#modelMenu")
      && !event.target.closest("#modelLevelMenu")) {
      closeModelMenu();
    }
    if (state.sessionMenuFor && !event.target.closest(".session-menu") && !event.target.closest(".session-menu-button")) closeSessionMenu();
    if (!elements.artifactResourceMenu.hidden && !event.target.closest(".artifact-resource-wrap")) closeArtifactResourceMenu();
  });
  elements.promptGrid.querySelectorAll("[data-prompt]").forEach((button) => {
    button.addEventListener("click", () => {
      if (elements.composerInput.disabled) return;
      elements.composerInput.value = button.dataset.prompt || "";
      resizeComposer();
      elements.composerInput.focus();
    });
  });
  elements.composerInput.addEventListener("input", resizeComposer);
  // 斜杠命令的补全菜单（逻辑在 commands.js，这里只喂输入、收回填）
  elements.composerInput.addEventListener("input", () => {
    window.GqyCommands?.onInput(elements.composerInput.value, elements.composerDock, (name) => {
      elements.composerInput.value = name;
      elements.composerInput.focus();
      resizeComposer();
    });
  });
  elements.composerInput.addEventListener("blur", () => window.GqyCommands?.hide());
  elements.attachButton.addEventListener("click", () => elements.attachmentInput.click());
  wireMicButton();
  elements.attachmentInput.addEventListener("change", () => {
    addComposerFiles(elements.attachmentInput.files);
    elements.attachmentInput.value = "";
  });
  elements.composerForm.addEventListener("dragenter", (event) => {
    if (!event.dataTransfer?.types?.includes("Files")) return;
    event.preventDefault();
    elements.composerForm.classList.add("is-dragging");
  });
  elements.composerForm.addEventListener("dragover", (event) => {
    if (!event.dataTransfer?.types?.includes("Files")) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
    elements.composerForm.classList.add("is-dragging");
  });
  elements.composerForm.addEventListener("dragleave", (event) => {
    if (!elements.composerForm.contains(event.relatedTarget)) elements.composerForm.classList.remove("is-dragging");
  });
  elements.composerForm.addEventListener("drop", (event) => {
    elements.composerForm.classList.remove("is-dragging");
    const files = collectTransferFiles(event.dataTransfer);
    if (!files.length) return;
    event.preventDefault();
    addComposerFiles(files);
  });
  elements.composerInput.addEventListener("paste", (event) => {
    const files = collectTransferFiles(event.clipboardData);
    if (!files.length) {
      const hasUriList = Array.from(event.clipboardData?.items || []).some((item) => item.type === "text/uri-list");
      if (hasUriList) showToast("浏览器没有提供文件内容，请直接拖入输入框", "error");
      return;
    }
    event.preventDefault();
    addComposerFiles(files);
  });
  elements.composerInput.addEventListener("compositionstart", () => {
    appState.composing = true;
  });
  elements.composerInput.addEventListener("compositionend", () => {
    appState.composing = false;
  });
  elements.composerInput.addEventListener("keydown", (event) => {
    // 菜单开着时它先吃掉上下键与 Tab/Enter：补全后再按一次回车才执行，
    // 与 REPL 一致，用户有机会反悔。
    if (window.GqyCommands?.handleKey(event)) {
      event.preventDefault();
      return;
    }
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing && !appState.composing && event.keyCode !== 229) {
      // 触屏设备上回车是换行:软键盘没有 Shift+Enter,回车即发送就没法
      // 打多行了。发送用按钮;Ctrl/Cmd+Enter 仍然发送。
      if (isTouchComposer() && !(event.ctrlKey || event.metaKey)) return;
      event.preventDefault();
      if (!elements.sendButton.disabled) elements.composerForm.requestSubmit();
    }
  });
  elements.composerForm.addEventListener("submit", (event) => {
    event.preventDefault();
    submitTurn();
  });
  elements.loginForm.addEventListener("submit", (event) => {
    event.preventDefault();
    submitLogin();
  });
  elements.setupForm.addEventListener("submit", (event) => {
    event.preventDefault();
    submitSetupAdmin();
  });
  elements.registerForm.addEventListener("submit", (event) => {
    event.preventDefault();
    submitRegister();
  });
  elements.showRegisterButton.addEventListener("click", () => showRegisterForm(true));
  elements.showLoginButton.addEventListener("click", () => showRegisterForm(false));
  elements.accountSave.addEventListener("click", saveAccount);
  elements.accountLogout.addEventListener("click", logout);
  elements.inviteCreate.addEventListener("click", createInvite);
  elements.personaCreate.addEventListener("click", () => openOobe({ reason: "create" }));
  bindOobeEvents();
  elements.newChatButton.addEventListener("click", requestNewConversation);
  elements.retryBootstrapButton.addEventListener("click", loadBootstrap);
  elements.resetConfirmButton.addEventListener("click", resetConversation);
  elements.chatScroll.addEventListener("scroll", () => {
    // 程序滚动的守卫由这条事件自己解除:以前用 setTimeout(0) 清,而 scroll
    // 事件要等到下一帧才派发,处理器等于裸跑,把一次跟随当成用户上滚关掉,
    // 下一帧又认为到底重新打开——来回翻转就是抖动的第二半。
    const programmatic = state.programmaticScroll;
    // 非 smooth:这一条事件就是那次滚动的回执,吃完即解除。
    if (programmatic && !programmaticScrollSmooth) {
      state.programmaticScroll = false;
      window.clearTimeout(programmaticScrollTimer);
    }
    state.nearBottom = isNearBottom();
    if (programmatic) return;
    if (!state.followOutput && isAtBottom()) {
      state.followOutput = true;
      elements.jumpBottomButton.hidden = true;
    } else if (!state.followOutput || !state.nearBottom) {
      suspendOutputFollowing();
    }
  }, { passive: true });
  elements.chatScroll.addEventListener("wheel", (event) => {
    if (event.deltaY < 0) suspendOutputFollowing();
  }, { passive: true });
  elements.chatScroll.addEventListener("touchmove", () => {
    suspendOutputFollowing();
  }, { passive: true });
  elements.jumpBottomButton.addEventListener("click", () => scrollToBottom({ force: true, smooth: true }));
  window.addEventListener("resize", () => {
    updateJumpButtonOffset();
    syncArtifactLayout();
    positionModelMenu();
  }, { passive: true });
  // 「回到底部」的 bottom 是按 composerDock 高度写的内联值。后台任务条
  // 出现/增行、软键盘顶起视口时 dock 会变高,但那些路径并不都经过
  // updateJumpButtonOffset,按钮就留在旧高度、压在任务条上——手机上一点
  // 就误触。直接盯 dock 的尺寸,谁改都跟上。
  if (typeof ResizeObserver === "function") {
    new ResizeObserver(() => updateJumpButtonOffset()).observe(elements.composerDock);
  }
  window.visualViewport?.addEventListener("resize", updateJumpButtonOffset, { passive: true });
  new ResizeObserver(syncArtifactLayout).observe(elements.mainStage);
  if (window.visualViewport) {
    window.visualViewport.addEventListener("resize", syncAppHeight, { passive: true });
    // iOS 只把可视视口平移、不改尺寸时不发 resize,只发 scroll。
    window.visualViewport.addEventListener("scroll", syncAppHeight, { passive: true });
    syncAppHeight();
  }
  document.addEventListener("keydown", handleGlobalKeydown);
}

function syncAppHeight() {
  const viewport = window.visualViewport;
  if (!viewport) return;
  document.documentElement.style.setProperty("--app-height", `${Math.round(viewport.height * viewport.scale / UI_SCALE)}px`);
  // 外壳缩到可视视口之后文档已经没得可滚,但 Safari 在键盘弹出的瞬间已经
  // 先滚过一次了,那段偏移要收回来,否则页面停在外壳底部的空白上。捏合放大
  // 时用户是在自己平移视口,这时不能抢方向盘。
  if (viewport.scale <= 1.01 && (window.scrollY || window.scrollX)) window.scrollTo(0, 0);
}

function initialize() {
  // 登录态没了(daemon 重启、令牌过期):直接回登录页,别等用户发消息时弹一句英文。
  onApiUnauthorized(() => {
    if (state.blocked) return false;
    showBlockedState(true, "", { expired: true });
    return true;
  });
  renderIconSlots();
  // 设置子页的默认值要先落定,深链再按 hash 覆盖,否则默认值会把深链盖掉。
  setSettingsView("interface");
  const deepLink = parseConsoleHash();
  if (deepLink) {
    if (deepLink.panel === "settings" && deepLink.view) setSettingsView(deepLink.view);
    if (deepLink.panel === "platforms" && deepLink.view) state.platformView = parsePlatformView(deepLink.view);
    consoleOpen(deepLink.panel);
  }
  setTheme(safeStorageGet("gqy.web.theme") || "graphite", false);
  const storedScheme = safeStorageGet("gqy.web.colorScheme");
  if (storedScheme) setColorScheme(storedScheme, false);
  probeMatugenTheme();
  setChatFontSize(safeStorageGet("gqy.web.chatFontSize") || "15px", false);
  setReasoningExpanded(safeStorageGet("gqy.web.reasoningExpanded") === "true", false);
  setToolExpanded(safeStorageGet("gqy.web.toolExpanded") === "true", false);
  // 没存过就是开(默认开),所以只认显式的 "false"
  setProcCollapse(safeStorageGet("gqy.web.procCollapse") !== "false", false);
  const artifactRatio = Number(safeStorageGet("gqy.web.artifactWidthRatio.v2"));
  if (Number.isFinite(artifactRatio) && artifactRatio >= 0.25 && artifactRatio <= 0.9) {
    state.artifactWidthRatio = artifactRatio;
  }
  setSidebarCollapsed(safeStorageGet("gqy.web.sidebarCollapsed") === "true");
  syncArtifactLayout();
  bindEvents();
  resizeComposer();
  updateSettingsControls();
  // 命令目录从服务端拉，前端不维护第二份清单。拉失败就当没有命令，
  // 所有 / 开头的输入照常发给模型。
  window.GqyCommands?.load(apiRequest);
  // 灯箱自己不会画图标（图标集在这边），把工厂函数递过去。
  window.GqyLightbox?.init({ makeIconSlot });
  window.GqyPreview?.init({ makeIconSlot, formatFileSize });
  window.GqyLinkCards?.init({ makeIconSlot, contentAdded });
  // 高亮和链接卡片的 settle 通道会在流停下来之后才改正文高度,那时已经没有
  // 下一条 delta 来触发滚动了,得让它们自己叫一声。
  window.GqyHighlight?.init({ contentAdded });
  startBrailleTicker();
  // G2:页面不可见时给 body 挂 gqy-paused,CSS 据此暂停全部装饰动画。
  // 实测(Xvfb+Chrome)不挂这个时隐藏窗口的合成负载与可见时完全一样。
  const syncPaused = () => document.body.classList.toggle("gqy-paused", document.hidden);
  document.addEventListener("visibilitychange", syncPaused);
  syncPaused();
  loadBootstrap();
}

start_core_ui_scale_js();
start_features_conversation_subagent_js();
start_features_jobs_js();
start_features_console_usage_js();

initialize();
