import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "components"

Panel {
  id: root
  moduleName: "io.github.akitwra.omarchy-drop"
  ipcTarget: "io.github.akitwra.omarchy-drop"

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  readonly property color foreground: bar ? bar.foreground : Color.foreground
  readonly property color urgent: bar ? bar.urgent : Color.urgent
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family
  readonly property int receiveSeconds: setting("discoverabilitySeconds", 600)

  property bool backendAvailable: false
  property string applicationState: "backend_missing"
  property var discoverability: null
  property var peers: []
  property var transfers: []
  property var adapters: []
  property var bluetooth: ({ available: false })
  property string message: "OmarchyDrop backend is not running."
  property string actionError: ""
  property double nowMillis: Date.now()

  readonly property bool receiving: discoverability !== null && discoverability.accepting_new_transfers === true
  readonly property bool usable: applicationState !== "hardware_unsupported" && applicationState !== "backend_missing" && applicationState !== "radio_unavailable" && applicationState !== "error"
  readonly property bool transferring: applicationState === "sending" || applicationState === "receiving"
  readonly property int remainingSeconds: {
    if (!discoverability || discoverability.expires_at_millis === null || discoverability.expires_at_millis === undefined) return -1
    return Math.max(0, Math.ceil((discoverability.expires_at_millis - nowMillis) / 1000))
  }
  readonly property string iconText: transferring ? (applicationState === "sending" ? "󰂯 ↑" : "󰂯 ↓") : (!backendAvailable || !usable ? "󰂯 !" : peers.length > 0 ? "󰂯 " + peers.length : "󰂯")
  readonly property string statusTitle: {
    if (!backendAvailable) return "Backend missing"
    if (applicationState === "hardware_unsupported") return "AirDrop unavailable"
    if (applicationState === "sending") return "Sending"
    if (applicationState === "receiving") return "Receiving"
    if (receiving) return "Ready to receive"
    return "Receiving is off"
  }
  readonly property string statusSubtitle: {
    if (actionError !== "") return actionError
    if (message !== "") return message
    if (receiving && remainingSeconds >= 0) return "Discoverable for " + formatDuration(remainingSeconds)
    if (receiving) return "Discoverable until turned off"
    return "Nearby devices cannot see this computer"
  }

  function formatDuration(seconds) {
    var minutes = Math.floor(seconds / 60)
    var rest = seconds % 60
    return (minutes < 10 ? "0" : "") + minutes + ":" + (rest < 10 ? "0" : "") + rest
  }

  function applyEvent(value) {
    if (!value || value.event !== "state" || !value.data) return
    var data = value.data
    backendAvailable = true
    applicationState = data.state || "error"
    discoverability = data.discoverability || null
    peers = data.peers || []
    transfers = data.transfers || []
    adapters = data.adapters || []
    bluetooth = data.bluetooth || ({ available: false })
    message = data.message || ""
  }

  function startEvents() {
    if (!eventProc.running) eventProc.running = true
  }

  Process {
    id: eventProc
    command: ["omdropctl", "events", "--jsonl"]
    stdout: SplitParser {
      onRead: function(line) {
        try { root.applyEvent(JSON.parse(line)) } catch (error) { root.actionError = "Backend sent invalid status data" }
      }
    }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: if (text.trim() !== "") root.actionError = text.trim()
    }
    onExited: {
      root.backendAvailable = false
      root.applicationState = "backend_missing"
      reconnectTimer.restart()
    }
  }

  Timer {
    id: reconnectTimer
    interval: 5000
    repeat: false
    onTriggered: root.startEvents()
  }

  Timer {
    interval: 1000
    repeat: true
    running: root.receiving
    onTriggered: root.nowMillis = Date.now()
  }

  Process {
    id: actionProc
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.actionError = text.trim()
    }
    onStarted: root.actionError = ""
  }

  function toggleReceiving() {
    if (actionProc.running || !backendAvailable) return
    actionProc.command = receiving
      ? ["omdropctl", "receive", "off"]
      : ["omdropctl", "receive", "on", "--seconds", String(receiveSeconds)]
    actionProc.running = true
  }

  function rescanHardware() {
    if (actionProc.running || !backendAvailable) return
    actionProc.command = ["omdropctl", "hardware", "probe", "--json"]
    actionProc.running = true
  }

  Component.onCompleted: startEvents()

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: root.iconText
    fontSize: Style.bar.iconFont
    centerFigures: false
    tooltipText: root.statusTitle + (root.transferring ? "" : "\n" + root.statusSubtitle)
    onPressed: root.toggle()
  }

  KeyboardPanel {
    id: panel
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(330))
    contentHeight: panel.fittedContentHeight(content.implicitHeight)

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onCloseRequested: root.controller.hide()
      onActivateRequested: root.toggleReceiving()

      Column {
        id: content
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        spacing: Style.space(12)

        StatusHeader {
          width: parent.width
          title: root.statusTitle
          subtitle: root.statusSubtitle
          glyph: root.transferring ? (root.applicationState === "sending" ? "↑" : "↓") : "󰂯"
          foreground: root.foreground
          accent: root.usable ? Color.accent : root.urgent
          fontFamily: root.fontFamily
        }

        Button {
          visible: root.backendAvailable && root.usable
          text: root.receiving ? "Turn receiving off" : "Everyone for " + Math.max(1, Math.round(root.receiveSeconds / 60)) + " minutes"
          bordered: true
          foreground: root.foreground
          enabled: !actionProc.running
          onClicked: root.toggleReceiving()
        }

        PanelSeparator { foreground: root.foreground }

        PanelSectionHeader {
          text: "NEARBY DEVICES"
          foreground: root.foreground
          fontFamily: root.fontFamily
        }

        Text {
          width: parent.width
          visible: root.peers.length === 0
          text: root.usable ? "No nearby Apple devices yet." : "Device discovery needs a validated AWDL adapter."
          textFormat: Text.PlainText
          color: Qt.darker(root.foreground, 1.5)
          font.family: root.fontFamily
          font.pixelSize: Style.font.bodySmall
          wrapMode: Text.WordWrap
        }

        Repeater {
          model: root.peers
          delegate: DeviceCard {
            required property var modelData
            width: content.width
            name: modelData.display_name || "Apple device"
            kind: modelData.device_class || "unknown"
            foreground: root.foreground
            fontFamily: root.fontFamily
            sendEnabled: false
          }
        }

        PanelSeparator { visible: root.transfers.length > 0; foreground: root.foreground }

        PanelSectionHeader {
          visible: root.transfers.length > 0
          text: "TRANSFERS"
          foreground: root.foreground
          fontFamily: root.fontFamily
        }

        Repeater {
          model: root.transfers
          delegate: TransferCard {
            required property var modelData
            width: content.width
            name: modelData.files && modelData.files.length > 0 ? modelData.files[0].name : "Transfer"
            detail: modelData.state + (modelData.peer_name ? " · " + modelData.peer_name : "")
            progress: modelData.bytes_total ? modelData.bytes_transferred / modelData.bytes_total : -1
            foreground: root.foreground
            fontFamily: root.fontFamily
          }
        }

        PanelSeparator { foreground: root.foreground }

        HardwareStatus {
          width: parent.width
          adapters: root.adapters
          foreground: root.foreground
          fontFamily: root.fontFamily
          onRescanRequested: root.rescanHardware()
        }

        Text {
          width: parent.width
          visible: !root.bluetooth.available
          text: "Bluetooth is unavailable. Receiving may still work; waking sleeping iPhones for sending will not."
          textFormat: Text.PlainText
          color: Qt.darker(root.foreground, 1.5)
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          wrapMode: Text.WordWrap
        }
      }
    }
  }
}
