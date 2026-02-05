package ui

import "testing"

func TestValidateInstanceName(t *testing.T) {
	existing := map[string]struct{}{
		"vanilla": {},
	}

	cases := []struct {
		name    string
		wantErr bool
	}{
		{name: "My Instance", wantErr: false},
		{name: "vanilla", wantErr: true},
		{name: "Vanilla", wantErr: true},
		{name: ".", wantErr: true},
		{name: "..", wantErr: true},
		{name: "bad/char", wantErr: true},
		{name: "bad\\char", wantErr: true},
		{name: "bad:char", wantErr: true},
		{name: "bad*char", wantErr: true},
		{name: "trailingspace ", wantErr: true},
		{name: "trailingperiod.", wantErr: true},
		{name: "contains\tcontrol", wantErr: true},
	}

	for _, tc := range cases {
		err := validateInstanceName(tc.name, existing)
		if tc.wantErr && err == nil {
			t.Fatalf("expected error for %q, got nil", tc.name)
		}
		if !tc.wantErr && err != nil {
			t.Fatalf("unexpected error for %q: %v", tc.name, err)
		}
	}
}
