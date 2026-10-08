import QtQuick
import qs.Commons
import qs.Ui

Column {
  id: root
  property var adapters: []
  property color foreground: Color.foreground
  property string fontFamily: Style.font.family
  signal rescanRequested()

  width: parent ? parent.width : implicitWidth
  spacing: Style.space(8)

  PanelSectionHeader {
    text: "HARDWARE"
    foreground: root.foreground
    fontFamily: root.fontFamily
  }

  Text {
    width: parent.width
    visible: root.adapters.length === 0
    text: "No Wi-Fi radio is visible. Connect a validated USB Wi-Fi adapter, then re-scan."
    textFormat: Text.PlainText
    color: Qt.darker(root.foreground, 1.35)
    font.family: root.fontFamily
    font.pixelSize: Style.font.bodySmall
    wrapMode: Text.WordWrap
  }

  Repeater {
    model: root.adapters
    delegate: Column {
      required property var modelData
      width: root.width
      spacing: Style.space(2)
      Text {
        width: parent.width
        text: (modelData.interface || modelData.id) + " · " + (modelData.driver || "unknown driver")
        textFormat: Text.PlainText
        color: root.foreground
        font.family: root.fontFamily
        font.pixelSize: Style.font.bodySmall
        font.bold: true
        elide: Text.ElideRight
      }
      Text {
        width: parent.width
        text: String(modelData.level || "unknown").split("_").join(" ")
        textFormat: Text.PlainText
        color: modelData.level === "supported" || modelData.level === "preferred" ? Color.accent : Qt.darker(root.foreground, 1.5)
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }
    }
  }

  Button {
    text: "Re-scan hardware"
    bordered: true
    foreground: root.foreground
    onClicked: root.rescanRequested()
  }
}
