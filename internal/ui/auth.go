package ui

import (
	"context"
	"fmt"
	"os/exec"
	"runtime"
	"time"

	"github.com/charmbracelet/bubbles/spinner"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
)

type AuthState int

const (
	AuthStateInit AuthState = iota
	AuthStateFetchingCode
	AuthStateWaitingForUser // Polling
	AuthStateExchange       // Swapping tokens
	AuthStateSuccess
	AuthStateError
)

type AuthModel struct {
	width  int
	height int

	state      AuthState
	deviceCode *api.DeviceCodeResponse
	err        error
	account    *core.Account
	copied     bool

	// pendingLaunch is the instance the user was trying to launch when this auth
	// screen opened (nil when opened from the Accounts menu). When set, the screen
	// offers an "[o] play offline" escape hatch that launches it without signing in.
	pendingLaunch *core.Instance

	spinner spinner.Model

	// Dependencies
	client   *api.AuthClient
	manager  *core.AccountManager
	clientID string
}

func NewAuthModel(dataDir string, clientID string, manager *core.AccountManager, pendingLaunch *core.Instance) *AuthModel {
	s := spinner.New()
	s.Spinner = spinner.Dot
	s.Style = lipgloss.NewStyle().Foreground(Active.Secondary)

	return &AuthModel{
		state:         AuthStateInit,
		spinner:       s,
		manager:       manager,
		clientID:      clientID,
		client:        api.NewAuthClient(clientID),
		pendingLaunch: pendingLaunch,
	}
}

func (m *AuthModel) Init() tea.Cmd {
	return tea.Batch(
		m.spinner.Tick,
		m.startAuthFlow, // Auto-start for now
	)
}

func (m *AuthModel) SetSize(w, h int) {
	m.width = w
	m.height = h
}

func (m *AuthModel) startAuthFlow() tea.Msg {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	dc, err := m.client.RequestDeviceCode(ctx)
	if err != nil {
		return errMsg{err: err}
	}
	return deviceCodeMsg{resp: dc}
}

func (m *AuthModel) pollToken(dc *api.DeviceCodeResponse) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
		defer cancel()

		token, err := m.client.PollForToken(ctx, dc)
		if err != nil {
			return errMsg{err: err}
		}
		return msaTokenMsg{resp: token}
	}
}

func (m *AuthModel) exchangeTokens(msaToken, refreshToken string) tea.Cmd {
	return func() tea.Msg {
		ctx := context.Background()

		// Xbox -> XSTS -> Minecraft (shared with the silent refresh flow).
		mcAccessToken, expiresIn, err := m.client.MinecraftLoginFromMSAToken(ctx, msaToken)
		if err != nil {
			return errMsg{err: err}
		}

		// Profile
		profile, err := m.client.FetchProfile(ctx, mcAccessToken)
		if err != nil {
			return errMsg{err: fmt.Errorf("fetch profile failed: %w", err)}
		}

		// Success! Persist the rotating MSA refresh token so the session can be
		// refreshed silently when the Minecraft token expires (~24h).
		acc := &core.Account{
			ID:              profile.ID,
			Name:            profile.Name,
			Type:            core.AccountTypeMSA,
			AccessToken:     mcAccessToken,
			ExpiresAt:       time.Now().Add(time.Duration(expiresIn) * time.Second),
			MSARefreshToken: refreshToken,
		}
		return accountCreatedMsg{acc: acc}
	}
}

func (m *AuthModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.MouseMsg:
		if msg.Type == tea.MouseLeft && m.state == AuthStateWaitingForUser && m.deviceCode != nil {
			copyToClipboard(m.deviceCode.UserCode)
			m.copied = true
			return m, tea.Tick(2*time.Second, func(_ time.Time) tea.Msg { return clearCopiedMsg{} })
		}

	case tea.KeyMsg:
		switch msg.String() {
		case "esc", "q":
			return m, func() tea.Msg { return NavigateToHome{} }
		case "o":
			// Escape hatch: play the pending instance offline instead of signing in.
			if m.pendingLaunch != nil {
				inst := m.pendingLaunch
				return m, func() tea.Msg { return NavigateToLaunch{Instance: inst, Offline: true} }
			}
		case "b":
			if m.state == AuthStateWaitingForUser && m.deviceCode != nil {
				openBrowser(m.deviceCode.VerificationURI)
			}
		case "c":
			if m.state == AuthStateWaitingForUser && m.deviceCode != nil {
				copyToClipboard(m.deviceCode.UserCode)
				m.copied = true
				return m, tea.Tick(2*time.Second, func(_ time.Time) tea.Msg { return clearCopiedMsg{} })
			}
		case "enter":
			if m.state == AuthStateSuccess {
				return m, func() tea.Msg { return NavigateToHome{} }
			}
		}

	case deviceCodeMsg:
		m.deviceCode = msg.resp
		m.state = AuthStateWaitingForUser
		// Auto-copy the code
		copyToClipboard(msg.resp.UserCode)
		m.copied = true
		// Schedule browser open after 1 second, also start polling, and schedule copied reset
		return m, tea.Batch(
			m.pollToken(msg.resp),
			tea.Tick(1*time.Second, func(_ time.Time) tea.Msg { return openBrowserMsg{} }),
			tea.Tick(3*time.Second, func(_ time.Time) tea.Msg { return clearCopiedMsg{} }),
		)

	case msaTokenMsg:
		m.state = AuthStateExchange
		// Thread the rotating MSA refresh token through so it lands on the
		// created account for later silent refreshes.
		return m, m.exchangeTokens(msg.resp.AccessToken, msg.resp.RefreshToken)

	case accountCreatedMsg:
		m.state = AuthStateSuccess
		m.account = msg.acc
		m.manager.Add(msg.acc)
		m.manager.Save()
		// Auto-close after 2s?
		return m, tea.Tick(2*time.Second, func(t time.Time) tea.Msg {
			return NavigateToHome{}
		})

	case errMsg:
		m.state = AuthStateError
		m.err = msg.err
		return m, nil // Wait for quit

	case spinner.TickMsg:
		var cmd tea.Cmd
		m.spinner, cmd = m.spinner.Update(msg)
		return m, cmd

	case openBrowserMsg:
		if m.deviceCode != nil {
			openBrowser(m.deviceCode.VerificationURI)
		}
		return m, nil

	case clearCopiedMsg:
		m.copied = false
		return m, nil
	}

	return m, nil
}

func (m *AuthModel) View() string {
	return lipgloss.Place(m.width, m.height, lipgloss.Center, lipgloss.Center, m.authCard())
}

// authCard renders the centered content for the current auth state.
func (m *AuthModel) authCard() string {
	w := m.width - 8
	if w > 60 {
		w = 60
	}
	if w < 24 {
		w = 24
	}

	switch m.state {
	case AuthStateInit, AuthStateFetchingCode:
		return authStatusLine(m.spinner.View(), "Contacting Microsoft…")

	case AuthStateExchange:
		return authStatusLine(m.spinner.View(), "Signing in to Minecraft…")

	case AuthStateSuccess:
		ok := lipgloss.NewStyle().Bold(true).Foreground(Active.Success).Render(GlyphDone + " Signed in")
		name := lipgloss.NewStyle().Foreground(Active.Text).Render("as " + m.account.Name)
		hint := lipgloss.NewStyle().Foreground(Active.TextMuted).Render("Returning to home…")
		return lipgloss.JoinVertical(lipgloss.Center, ok, name, "", hint)

	case AuthStateError:
		bad := lipgloss.NewStyle().Bold(true).Foreground(Active.Error).Render(GlyphFail + " Sign-in failed")
		body := lipgloss.NewStyle().Foreground(Active.TextSubtle).Render(fmt.Sprintf("%v", m.err))
		panel := Panel("Error", body, w, Active.Error)
		errHints := []KeyHint{{"esc", "back"}}
		if m.pendingLaunch != nil {
			errHints = append([]KeyHint{{"o", "play offline"}}, errHints...)
		}
		hints := KeyHints(w, errHints...)
		return lipgloss.JoinVertical(lipgloss.Left, bad, "", panel, "", hints)

	case AuthStateWaitingForUser:
		if m.deviceCode == nil {
			return authStatusLine("", "No device code — press [esc] to go back.")
		}
		return m.authDeviceCard(w)
	}
	return ""
}

// authDeviceCard renders the two-step device-code sign-in flow.
func (m *AuthModel) authDeviceCard(w int) string {
	header := ScreenHeader("Microsoft sign-in", "Authorize mctui to use your Minecraft account")

	url := lipgloss.NewStyle().Foreground(Active.SuccessAccent).Underline(true).Render(m.deviceCode.VerificationURI)
	step1 := authStep("1", "Open this page in your browser", url)

	code := lipgloss.NewStyle().Bold(true).Foreground(Active.Primary).Render(m.deviceCode.UserCode)
	copyState := lipgloss.NewStyle().Foreground(Active.TextMuted).Render("press [c] to copy")
	if m.copied {
		copyState = lipgloss.NewStyle().Foreground(Active.Success).Render(GlyphDone + " copied")
	}
	step2 := authStep("2", "Enter this device code", code+"   "+copyState)

	body := lipgloss.JoinVertical(lipgloss.Left, step1, "", step2)
	panel := Panel("Sign in", body, w, Active.Primary)

	waiting := lipgloss.NewStyle().
		Foreground(Active.TextSubtle).
		Render(m.spinner.View() + " Waiting for you to sign in…")
	cardHints := []KeyHint{{"c", "copy code"}, {"b", "open browser"}}
	if m.pendingLaunch != nil {
		cardHints = append(cardHints, KeyHint{"o", "play offline"})
	}
	cardHints = append(cardHints, KeyHint{"esc", "back"})
	hints := KeyHints(w, cardHints...)

	return lipgloss.JoinVertical(lipgloss.Left, header, "", panel, "", waiting, "", hints)
}

// authStatusLine is a spinner + message for transient auth states.
func authStatusLine(spinner, msg string) string {
	if spinner != "" {
		msg = spinner + " " + msg
	}
	return lipgloss.NewStyle().Foreground(Active.TextSubtle).Render(msg)
}

// authStep renders a numbered step with a detail line beneath it.
func authStep(num, title, detail string) string {
	n := lipgloss.NewStyle().Bold(true).Foreground(Active.Secondary).Render(num)
	t := lipgloss.NewStyle().Foreground(Active.Text).Render(title)
	d := lipgloss.NewStyle().Foreground(Active.TextSubtle).Render("   " + detail)
	return lipgloss.JoinVertical(lipgloss.Left, n+"  "+t, d)
}

// Messages
type deviceCodeMsg struct{ resp *api.DeviceCodeResponse }
type msaTokenMsg struct{ resp *api.MSATokenResponse }
type accountCreatedMsg struct{ acc *core.Account }
type errMsg struct{ err error }

func openBrowser(url string) {
	_ = openURL(url)
}

func copyToClipboard(text string) error {
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "darwin":
		cmd = exec.Command("pbcopy")
	case "linux":
		// Try wl-copy first, then xclip
		if _, err := exec.LookPath("wl-copy"); err == nil {
			cmd = exec.Command("wl-copy")
		} else {
			cmd = exec.Command("xclip", "-selection", "clipboard")
		}
	default:
		return fmt.Errorf("unsupported platform")
	}

	in, err := cmd.StdinPipe()
	if err != nil {
		return err
	}

	if err := cmd.Start(); err != nil {
		return err
	}

	if _, err := in.Write([]byte(text)); err != nil {
		return err
	}
	if err := in.Close(); err != nil {
		return err
	}

	return cmd.Wait()
}

type clearCopiedMsg struct{}
type openBrowserMsg struct{}
