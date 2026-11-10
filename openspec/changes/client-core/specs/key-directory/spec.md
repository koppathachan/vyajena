## ADDED Requirements

### Requirement: Last-resort KeyPackage refresh
Each device SHALL replace its last-resort KeyPackage on the Pi when the current
one is older than 7 days, and SHALL upload more one-time KeyPackages whenever it
receives a low-KeyPackages notice.

#### Scenario: Weekly refresh
- **WHEN** Bob's last-resort KeyPackage is 7 days old
- **THEN** his core uploads a new last-resort KeyPackage and the Pi hands out the
  new one from then on
