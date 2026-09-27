package ui

import (
	"strings"
	"testing"

	"github.com/aayushdutt/mctui/internal/core"
	"github.com/charmbracelet/x/ansi"
)

// Deleting an instance wipes its folder (worlds included): every way of opening
// the confirm must focus Cancel, so d then Enter does nothing.
func TestHome_DeleteConfirmDefaultsToCancel(t *testing.T) {
	restoreDark(t)
	inst := &core.Instance{ID: "a", Name: "Survival", Version: "1.21.4", Path: t.TempDir(), PlayTime: 3*3600 + 12*60}
	arm := map[string]func(m *HomeModel){
		"d key":   func(m *HomeModel) { m.Update(keyMsg("d")) },
		"x popup": func(m *HomeModel) { m.openActionsPopup(inst); m.Update(keyMsg("d")) },
		"palette": func(m *HomeModel) { m.paletteDeleteInstance() },
	}
	for name, open := range arm {
		t.Run(name, func(t *testing.T) {
			m := NewHomeModel()
			m.SetSize(120, 40)
			m.SetInstances([]*core.Instance{inst}, "a")
			open(m)
			if !m.confirmDelete || m.deleteFocusYes {
				t.Fatalf("confirm open=%v focusYes=%v, want open on Cancel", m.confirmDelete, m.deleteFocusYes)
			}
			if _, cmd := m.Update(keyMsg("enter")); cmd != nil {
				t.Errorf("Enter on the default must not delete (got %T)", cmd())
			}
		})
	}

	m := NewHomeModel()
	m.SetSize(120, 40)
	m.SetInstances([]*core.Instance{inst}, "a")
	m.Update(keyMsg("d"))
	if view := ansi.Strip(m.View()); !strings.Contains(view, "3h 12m played") {
		t.Errorf("the dialog should say what's lost (playtime); got:\n%s", view)
	}
	m.Update(keyMsg("left"))
	_, cmd := m.Update(keyMsg("enter"))
	if cmd == nil {
		t.Fatal("Enter on the highlighted Delete should delete")
	}
	if _, ok := cmd().(DeleteInstance); !ok {
		t.Errorf("got %T, want DeleteInstance", cmd())
	}
}

func TestMods_RemoveConfirmDefaultsToCancel(t *testing.T) {
	m := newModsWith(t)
	m.modsFocus = panelInstalled
	m, _ = sendMods(t, m, "d")
	if m.modsDialog != modsDialogConfirmRemoveJar || m.modsDialogFocusYes {
		t.Fatalf("remove dialog should open on Cancel (dialog=%d focusYes=%v)", m.modsDialog, m.modsDialogFocusYes)
	}
	before := len(m.installed.Items())
	m, _ = sendMods(t, m, "enter")
	if len(m.installed.Items()) != before {
		t.Error("Enter on the default must not remove the jar")
	}
}

// "Clear your pack?" highlighted Keep but Enter cleared anyway.
func TestRP_ClearDialogEnterFollowsHighlight(t *testing.T) {
	restoreDark(t)
	m := rpRenderModel(t, 120, 40, map[string][]string{"Aesthetic": {"ClearGlass"}})
	m.inst.Path = t.TempDir() // clearing saves the cart under the instance

	m.Update(keyMsg("c"))
	if m.rpDialog != rpDialogConfirmClear || m.rpDialogFocusYes {
		t.Fatalf("clear dialog should open on Keep (dialog=%d focusYes=%v)", m.rpDialog, m.rpDialogFocusYes)
	}
	m.Update(keyMsg("enter"))
	if m.sel.Count() != 1 || m.rpDialog != rpDialogNone {
		t.Fatalf("Enter on Keep must close without clearing (count=%d)", m.sel.Count())
	}

	m.Update(keyMsg("c"))
	m.Update(keyMsg("left"))
	m.Update(keyMsg("enter"))
	if m.sel.Count() != 0 {
		t.Errorf("Enter on the highlighted Clear should clear (count=%d)", m.sel.Count())
	}
}

// Enter in "Your pack" is affirmative-only: it must not remove the tweak
// (space or d do).
func TestRP_EnterInCartDoesNotRemove(t *testing.T) {
	restoreDark(t)
	m := rpRenderModel(t, 120, 40, map[string][]string{"Aesthetic": {"ClearGlass"}})
	m.inst.Path = t.TempDir()
	m.rpFocus = rpPanelCart
	m.Update(keyMsg("enter"))
	if m.sel.Count() != 1 {
		t.Fatalf("enter removed a tweak from the pack (count=%d)", m.sel.Count())
	}
	m.Update(keyMsg("d"))
	if m.sel.Count() != 0 {
		t.Errorf("d in the pack pane should remove the tweak (count=%d)", m.sel.Count())
	}
}

// Esc during Build & Apply cancels it and stays on the screen.
func TestRP_EscDuringBuildCancelsInPlace(t *testing.T) {
	restoreDark(t)
	m := rpRenderModel(t, 120, 40, map[string][]string{"Aesthetic": {"ClearGlass"}})
	m.applying = true
	_, cmd := m.Update(keyMsg("esc"))
	if cmd != nil {
		if _, home := cmd().(NavigateToHome); home {
			t.Fatal("esc during a build left the screen")
		}
	}
	if m.applying {
		t.Error("esc should cancel the build")
	}
	if f := ansi.Strip(KeyHints(200, m.footerHelpItems()...)); strings.Contains(f, "esc cancel build") {
		t.Error("after cancelling, the footer should be back to normal")
	}
}
