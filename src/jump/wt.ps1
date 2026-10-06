$ErrorActionPreference = 'Stop'
Add-Type -Namespace Abtop -Name Native -MemberDefinition @'
[DllImport("kernel32.dll")] public static extern bool FreeConsole();
[DllImport("kernel32.dll")] public static extern bool AttachConsole(uint pid);
[DllImport("kernel32.dll", CharSet = CharSet.Unicode)] public static extern uint GetConsoleTitleW(System.Text.StringBuilder title, uint size);
[DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow();
[DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr hwnd);
[DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hwnd);
[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd, int cmd);
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
'@
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$native = [Abtop.Native]
$uia = [System.Windows.Automation.AutomationElement]

# Agents animate a leading status glyph several times a second.
function Normalize($s) { $s -replace '^[^\p{L}\p{N}]+', '' }
if ($env:ABTOP_TARGET_TAB_TITLE) {
    $title = Normalize $env:ABTOP_TARGET_TAB_TITLE
    $isTerminal = New-Object System.Windows.Automation.PropertyCondition($uia::ClassNameProperty, 'CASCADIA_HOSTING_WINDOW_CLASS')
    $windows = @($uia::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $isTerminal))
} else {
    # A console title is readable only while attached to that console.
    [void]$native::FreeConsole()
    if (-not $native::AttachConsole($targetPid)) { 'NOTAPPLICABLE'; exit }
    $buf = New-Object System.Text.StringBuilder 1024
    [void]$native::GetConsoleTitleW($buf, 1024)
    # Windows Terminal parents each pane's hidden ConPTY window to its host window.
    $hwnd = $native::GetParent($native::GetConsoleWindow())
    [void]$native::FreeConsole()
    if ($hwnd -eq [IntPtr]::Zero) { 'NOTAPPLICABLE'; exit }
    $window = $uia::FromHandle($hwnd)
    if ($window.Current.ClassName -ne 'CASCADIA_HOSTING_WINDOW_CLASS') { 'NOTAPPLICABLE'; exit }
    $windows = @($window)
    $title = Normalize $buf.ToString()
}
$isTab = New-Object System.Windows.Automation.PropertyCondition($uia::ControlTypeProperty, [System.Windows.Automation.ControlType]::TabItem)
$matches = @(
    foreach ($window in $windows) {
        foreach ($tab in $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, $isTab)) {
            if ($title -and (Normalize $tab.Current.Name) -ceq $title) {
                [PSCustomObject]@{ Tab = $tab; Window = $window }
            }
        }
    }
)
if ($matches.Count -eq 0) { 'NOMATCH'; exit }
if ($matches.Count -gt 1) { 'AMBIGUOUS'; exit }
$matches[0].Tab.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
$hwnd = [IntPtr]$matches[0].Window.Current.NativeWindowHandle
if ($native::IsIconic($hwnd)) { [void]$native::ShowWindow($hwnd, 9) }
[void]$native::SetForegroundWindow($hwnd)
'JUMPED'
