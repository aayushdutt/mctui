package ui

import (
	"context"
	"errors"
	"fmt"
	"path/filepath"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/aayushdutt/mctui/internal/mods"
)

// Update implements tea.Model.
func (m *ModsModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	if m.blocked {
		switch msg := msg.(type) {
		case tea.KeyMsg:
			if msg.String() == "esc" || msg.String() == "q" {
				m.CancelPending()
				return m, func() tea.Msg { return NavigateToHome{} }
			}
		}
		return m, nil
	}

	switch msg := msg.(type) {
	case tea.MouseMsg:
		if m.installing {
			return m, nil
		}
		switch msg.Type {
		case tea.MouseWheelUp:
			switch m.modsFocus {
			case panelInstalled:
				m.clearModsDialog()
				m.libraryToast = ""
				if len(m.installed.Items()) > 0 {
					m.installed.CursorUp()
				}
			case panelBrowse:
				if len(m.results.Items()) > 0 {
					m.results.CursorUp()
				}
			case panelQuery:
				if len(m.results.Items()) > 0 {
					m.results.CursorUp()
				}
			}
		case tea.MouseWheelDown:
			switch m.modsFocus {
			case panelInstalled:
				m.clearModsDialog()
				m.libraryToast = ""
				if len(m.installed.Items()) > 0 {
					m.installed.CursorDown()
				}
			case panelBrowse:
				if len(m.results.Items()) > 0 {
					m.results.CursorDown()
				}
			case panelQuery:
				if len(m.results.Items()) > 0 {
					m.results.CursorDown()
				}
			}
		default:
			return m, nil
		}
		return m, nil

	case tea.KeyMsg:
		if m.installing && msg.String() != "esc" {
			return m, nil
		}

		if m.modsDialog == modsDialogConfirmRemoveJar {
			switch msg.String() {
			case "y", "Y", "enter":
				jar := m.modsDialogJar
				m.clearModsDialog()
				m.removeConfirmedJar(jar)
				return m, nil
			case "n", "N", "esc", "backspace":
				m.clearModsDialog()
				return m, nil
			default:
				return m, nil
			}
		}

		switch msg.String() {
		case "esc":
			m.CancelPending()
			if m.installing {
				m.installing = false
				m.installingProjectID = ""
				m.rebuildBrowseBadges()
			}
			return m, func() tea.Msg { return NavigateToHome{} }
		case "tab", "shift+tab":
			m.libraryToast = ""
			m.cycleTab()
			if m.modsFocus == panelQuery {
				return m, textinput.Blink
			}
			return m, nil
		case "r", "R":
			// Let query field receive "r" (e.g. sodium, fabric-apiR…).
			if m.modsFocus != panelQuery {
				m.refreshInstalled()
				return m, nil
			}
		case "d":
			if m.modsFocus == panelInstalled && m.openRemoveConfirmDialog() {
				return m, nil
			}
		case "backspace":
			if m.modsFocus == panelInstalled && m.openRemoveConfirmDialog() {
				return m, nil
			}
		case "/":
			// Typing "/" in the search box should not jump/refocus the field.
			if m.modsFocus != panelQuery {
				m.libraryToast = ""
				m.modsFocus = panelQuery
				m.query.Focus()
				return m, textinput.Blink
			}
		}

		switch msg.String() {
		case "left":
			if m.modsFocus == panelBrowse || m.modsFocus == panelQuery {
				m.libraryToast = ""
				m.modsFocus = panelInstalled
				m.query.Blur()
				return m, nil
			}
			if m.modsFocus == panelInstalled {
				m.clearModsDialog()
				m.libraryToast = ""
			}
		case "right":
			if m.modsFocus == panelInstalled {
				m.clearModsDialog()
				m.libraryToast = ""
				m.modsFocus = panelBrowse
				return m, nil
			}
		}

		if m.modsFocus == panelQuery && msg.String() == "down" {
			if len(m.results.Items()) > 0 {
				m.modsFocus = panelBrowse
				m.query.Blur()
				return m, nil
			}
		}
		if m.modsFocus == panelQuery && msg.String() == "up" {
			m.modsFocus = panelInstalled
			m.query.Blur()
			if len(m.installed.Items()) > 0 {
				m.installed.Select(len(m.installed.Items()) - 1)
			}
			return m, nil
		}

		if m.modsFocus == panelQuery && msg.String() == "enter" {
			m.cancelSearch()
			m.searchSeq++
			if strings.TrimSpace(m.query.Value()) == "" {
				m.results.Title = "Modrinth — popular"
			} else {
				m.results.Title = "Modrinth — search"
			}
			m.searchNotice = ""
			return m, m.runSearchQuery()
		}

		if m.modsFocus == panelQuery {
			old := m.query.Value()
			var cmd tea.Cmd
			m.query, cmd = m.query.Update(msg)
			if m.query.Value() != old {
				m.cancelSearch()
				m.searchSeq++
				seq := m.searchSeq
				if strings.TrimSpace(m.query.Value()) == "" {
					m.results.Title = "Modrinth — popular"
				} else {
					m.results.Title = "Modrinth — search"
				}
				m.searchNotice = ""
				return m, tea.Batch(cmd, tea.Tick(350*time.Millisecond, func(time.Time) tea.Msg {
					return modSearchDueMsg{seq: seq}
				}))
			}
			return m, cmd
		}

		if m.modsFocus == panelBrowse && (msg.String() == "up" || msg.String() == "k") && m.results.Index() == 0 {
			m.modsFocus = panelQuery
			m.query.Focus()
			return m, textinput.Blink
		}

		if m.modsFocus == panelInstalled && msg.String() == "up" && m.installed.Index() == 0 {
			m.clearModsDialog()
			m.libraryToast = ""
			m.modsFocus = panelQuery
			m.query.Focus()
			return m, textinput.Blink
		}
		if m.modsFocus == panelInstalled && msg.String() == "down" {
			n := len(m.installed.Items())
			if n > 0 && m.installed.Index() >= n-1 {
				m.clearModsDialog()
				m.libraryToast = ""
				m.modsFocus = panelBrowse
				return m, nil
			}
		}

		if m.modsFocus == panelInstalled && key.Matches(msg, key.NewBinding(key.WithKeys("enter"))) {
			if m.openRemoveConfirmDialog() {
				return m, nil
			}
		}

		if m.modsFocus == panelBrowse && key.Matches(msg, key.NewBinding(key.WithKeys("enter"))) {
			if it, ok := m.results.SelectedItem().(modListItem); ok {
				if m.installing {
					m.installOK = ""
					m.installErr = ""
					m.searchNotice = "A download is already in progress."
					return m, nil
				}
				if it.rowNote == modRowInstalled ||
					mods.IsModrinthProjectInstalled(m.catalog, m.cachedJars, it.hit.ProjectID, it.hit.Slug) {
					m.installOK = ""
					m.installErr = ""
					m.searchNotice = "Already installed — skipped."
					return m, nil
				}
				m.installingProjectID = it.hit.ProjectID
				m.rebuildBrowseBadges()
				return m, m.installModCmd(it.hit.ProjectID, it.hit.Title, it.hit.Slug)
			}
			return m, nil
		}

		var cmd tea.Cmd
		switch m.modsFocus {
		case panelInstalled:
			m.installed, cmd = m.installed.Update(msg)
			m.syncModsDialogSelection()
		case panelBrowse:
			m.results, cmd = m.results.Update(msg)
		}
		return m, cmd

	case modSearchDueMsg:
		if msg.seq != m.searchSeq {
			return m, nil
		}
		return m, m.runSearchQuery()

	case modSearchStaleMsg:
		return m, nil

	case modSearchResultMsg:
		if msg.seq != m.searchSeq {
			return m, nil
		}
		m.searching = false
		m.searchCancel = nil
		if msg.err != nil {
			if errors.Is(msg.err, context.Canceled) {
				return m, nil
			}
			m.searchErr = msg.err.Error()
			m.searchNotice = ""
			m.lastTotalHits = 0
			m.results.SetItems(nil)
			return m, nil
		}
		m.searchErr = ""
		if msg.result == nil {
			m.searchNotice = ""
			m.lastTotalHits = 0
			m.results.SetItems(nil)
			return m, nil
		}
		m.lastTotalHits = msg.result.TotalHits
		m.results.SetItems(m.hitsToItems(msg.result.Hits))
		if len(msg.result.Hits) == 0 {
			m.searchNotice = "No matches. Clear the search for popular picks on this version."
		} else {
			m.searchNotice = ""
		}
		if strings.TrimSpace(m.query.Value()) == "" {
			m.results.Title = "Modrinth — popular"
		} else {
			m.results.Title = "Modrinth — search"
		}
		return m, nil

	case ModInstallDoneMsg:
		m.installing = false
		m.installingProjectID = ""
		m.cancelInstallDownload()
		m.installErr = ""
		m.installOK = ""
		if msg.Err != nil {
			if errors.Is(msg.Err, context.Canceled) {
				m.rebuildBrowseBadges()
				return m, nil
			}
			m.installErr = msg.Err.Error()
			m.rebuildBrowseBadges()
			return m, nil
		}
		if err := mods.RecordModrinthInstall(m.inst, msg.ProjectID, msg.Slug, filepath.Base(msg.Path)); err != nil {
			m.installErr = fmt.Sprintf("Saved jar but catalog: %v", err)
		}
		m.reloadCatalogAndJars()
		m.refreshInstalled()
		m.rebuildBrowseBadges()
		short := filepath.Base(msg.Path)
		m.installOK = fmt.Sprintf("Added %s (%s) ✓  — restart Minecraft to load", msg.Title, short)
		return m, nil
	}

	var cmd tea.Cmd
	switch m.modsFocus {
	case panelQuery:
		m.query, cmd = m.query.Update(msg)
	case panelInstalled:
		m.installed, cmd = m.installed.Update(msg)
		m.syncModsDialogSelection()
	case panelBrowse:
		m.results, cmd = m.results.Update(msg)
	}
	return m, cmd
}
