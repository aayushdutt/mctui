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
	
	"github.com/quasar/mctui/internal/api"
	"github.com/quasar/mctui/internal/core"
)

type AuthState int

const (
	AuthStateInit AuthState = iota
	AuthStateFetchingCode
	AuthStateWaitingForUser // Polling
	AuthStateExchange // Swapping tokens
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

	spinner spinner.Model

	// Dependencies
	client   *api.AuthClient
	manager  *core.AccountManager
	clientID string
}

func NewAuthModel(dataDir string, clientID string, manager *core.AccountManager) *AuthModel {
	s := spinner.New()
	s.Spinner = spinner.Dot
	s.Style = lipgloss.NewStyle().Foreground(lipgloss.Color("205"))

	return &AuthModel{
		state:    AuthStateInit,
		spinner:  s,
		manager:  manager,
		clientID: clientID,
		client:   api.NewAuthClient(clientID),
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

func (m *AuthModel) exchangeTokens(msaToken string) tea.Cmd {
	return func() tea.Msg {
		ctx := context.Background()

		// 1. Xbox
		xboxResp, err := m.client.AuthenticateXbox(ctx, msaToken)
		if err != nil {
			return errMsg{err: fmt.Errorf("xbox auth failed: %w", err)}
		}

		// 2. XSTS
		xstsResp, err := m.client.AuthenticateXSTS(ctx, xboxResp.Token)
		if err != nil {
			return errMsg{err: fmt.Errorf("xsts auth failed: %w", err)}
		}

		// 3. Minecraft
		uhs := xstsResp.DisplayClaims.XUI[0].UHS
		mcResp, err := m.client.LoginWithXbox(ctx, uhs, xstsResp.Token)
		if err != nil {
			return errMsg{err: fmt.Errorf("minecraft login failed: %w", err)}
		}

		// 4. Profile
		profile, err := m.client.FetchProfile(ctx, mcResp.AccessToken)
		if err != nil {
			return errMsg{err: fmt.Errorf("fetch profile failed: %w", err)}
		}

		// Success!
		acc := &core.Account{
			ID:          profile.ID,
			Name:        profile.Name,
			Type:        core.AccountTypeMSA,
			AccessToken: mcResp.AccessToken,
			ExpiresAt:   time.Now().Add(time.Duration(mcResp.ExpiresIn) * time.Second),
			// Refresh token from MSA? The PollForToken returns it.
			// But I need to thread it through. 
			// For now, MVP assumes success.
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
		// Store refresh token temporarily or pass it?
		// For MVP, just proceed.
		return m, m.exchangeTokens(msg.resp.AccessToken)

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
	doc := lipgloss.NewStyle().Padding(2, 4).Width(m.width).Height(m.height)
	
	var content string

	switch m.state {
	case AuthStateInit, AuthStateFetchingCode:
		content = fmt.Sprintf("%s Contacting Microsoft...", m.spinner.View())

	case AuthStateWaitingForUser:
		if m.deviceCode == nil {
			content = "Error: No device code."
		} else {
			codeText := m.deviceCode.UserCode
			if m.copied {
				codeText += "  ✓ Copied!"
			} else {
				codeText += "  📋"
			}
			
			box := lipgloss.NewStyle().
				Border(lipgloss.RoundedBorder()).
				BorderForeground(lipgloss.Color("63")).
				Padding(1, 2).
				Render(codeText)

			actionText := "[c] Copy code"
			if m.copied {
				actionText = "[✓] Copied!"
			}

			content = fmt.Sprintf(`
%s

To sign in, use a web browser to open the page:
%s

And enter the code:
%s

%s Waiting for you to sign in...
%s • [o] Open browser automatically
`, "Microsoft Authentication",
				lipgloss.NewStyle().Foreground(lipgloss.Color("86")).Render(m.deviceCode.VerificationURI),
				box,
				m.spinner.View(),
				actionText)
		}

	case AuthStateExchange:
		content = fmt.Sprintf("%s Logging in to Minecraft...", m.spinner.View())

	case AuthStateSuccess:
		content = fmt.Sprintf("✅ Successfully logged in as %s!\n\nRedirecting...", m.account.Name)

	case AuthStateError:
		content = fmt.Sprintf("❌ Error: %v\n\n[Esc] Back", m.err)
	}

	return doc.Render(content)
}

// Messages
type deviceCodeMsg struct { resp *api.DeviceCodeResponse }
type msaTokenMsg struct { resp *api.MSATokenResponse }
type accountCreatedMsg struct { acc *core.Account }
type errMsg struct { err error }

func openBrowser(url string) {
	var err error
	switch runtime.GOOS {
	case "linux":
		err = exec.Command("xdg-open", url).Start()
	case "windows":
		err = exec.Command("rundll32", "url.dll,FileProtocolHandler", url).Start()
	case "darwin":
		err = exec.Command("open", url).Start()
	default:
		err = fmt.Errorf("unsupported platform")
	}
	if err != nil {
		// handle error?
	}
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
