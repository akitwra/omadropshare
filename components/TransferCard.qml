import QtQuick
import qs.Commons

Item {
  id: root
  property string name: "Transfer"
  property string detail: ""
  property real progress: -1
  property color foreground: Color.foreground
  property string fontFamily: Style.font.family

  implicitHeight: labels.implicitHeight + (progress >= 0 ? Style.space(7) : 0)

  Column {
    id: labels
    width: parent.width
    spacing: Style.space(2)
    Text {
      width: parent.width
      text: root.name
      textFormat: Text.PlainText
      color: root.foreground
      font.family: root.fontFamily
      font.pixelSize: Style.font.bodySmall
      elide: Text.ElideMiddle
    }
    Text {
      width: parent.width
      text: root.detail
      textFormat: Text.PlainText
      color: Qt.darker(root.foreground, 1.5)
      font.family: root.fontFamily
      font.pixelSize: Style.font.caption
      elide: Text.ElideRight
    }
    Rectangle {
      visible: root.progress >= 0
      width: parent.width
      height: Style.space(3)
      radius: height / 2
      color: Qt.darker(root.foreground, 2.2)
      Rectangle {
        width: parent.width * Math.max(0, Math.min(1, root.progress))
        height: parent.height
        radius: parent.radius
        color: Color.accent
      }
    }
  }
}
