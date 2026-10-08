import QtQuick
import qs.Commons
import qs.Ui

Item {
  id: root
  property string name: "iPhone"
  property string kind: "unknown"
  property color foreground: Color.foreground
  property string fontFamily: Style.font.family
  property bool sendEnabled: false
  signal sendRequested()

  implicitHeight: Math.max(labels.implicitHeight, sendButton.implicitHeight)

  Column {
    id: labels
    anchors.left: parent.left
    anchors.right: sendButton.left
    anchors.rightMargin: Style.space(10)
    anchors.verticalCenter: parent.verticalCenter
    spacing: Style.space(1)
    Text {
      width: parent.width
      text: root.name
      textFormat: Text.PlainText
      color: root.foreground
      font.family: root.fontFamily
      font.pixelSize: Style.font.body
      elide: Text.ElideRight
    }
    Text {
      text: root.kind === "iphone" ? "iPhone" : root.kind === "ipad" ? "iPad" : root.kind === "mac" ? "Mac" : "Apple device"
      textFormat: Text.PlainText
      color: Qt.darker(root.foreground, 1.55)
      font.family: root.fontFamily
      font.pixelSize: Style.font.caption
    }
  }

  Button {
    id: sendButton
    anchors.right: parent.right
    anchors.verticalCenter: parent.verticalCenter
    text: "Send"
    bordered: true
    foreground: root.foreground
    enabled: root.sendEnabled
    onClicked: root.sendRequested()
  }
}
