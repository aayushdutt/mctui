// Package ui provides TUI view messages shared between components.
package ui

import (
	"github.com/quasar/mctui/internal/core"
	"github.com/quasar/mctui/internal/launch"
)

// Navigation messages
type (
	// NavigateToHome returns to the home screen
	NavigateToHome struct{}

	// NavigateToNewInstance opens the new instance wizard
	NavigateToNewInstance struct{}

	// NavigateToMods opens the mod browser
	NavigateToMods struct {
		Instance *core.Instance
	}

	// NavigateToSettings opens settings
	NavigateToSettings struct{}

	// NavigateToLaunch starts the launch view
	NavigateToLaunch struct {
		Instance *core.Instance
		Offline  bool
	}

	// NavigateToAuth opens the authentication screen
	NavigateToAuth struct{}

	// DeleteInstance requests instance deletion
	DeleteInstance struct {
		Instance *core.Instance
	}
)

// Action messages
type (
	// InstanceCreated is sent when a new instance is created
	InstanceCreated struct {
		Instance *core.Instance
	}

	// InstancesLoaded is sent when instances are loaded from disk
	InstancesLoaded struct {
		Instances []*core.Instance
		Error     error
	}

	// VersionsLoaded is sent when version manifest is fetched
	VersionsLoaded struct {
		Versions []core.Version
		Latest   string
		Error    error
	}

	// VersionDetailsLoaded is sent when full version info is fetched
	VersionDetailsLoaded struct {
		Details *core.VersionDetails
		Error   error
	}

	// LaunchStatusUpdate is sent during launch
	LaunchStatusUpdate struct {
		Status launch.Status
	}

	// LaunchComplete is sent when launch finishes
	LaunchComplete struct {
		Error error
	}
)
