import QtQuick
import QtQuick.Layouts
import qs.Commons

Item {
  id: root
  property string title: "OmarchyDrop"
  property string subtitle: ""
  property string glyph: "󰂯"
  property color foreground: Color.foreground
  property color accent: Color.accent
  property string fontFamily: Style.font.family

  implicitHeight: Math.max(icon.implicitHeight, labels.implicitHeight)

  Text {
    id: icon
    anchors.left: parent.left
    anchors.verticalCenter: parent.verticalCenter
    text: root.glyph
    textFormat: Text.PlainText
    color: root.accent
    font.family: root.fontFamily
    font.pixelSize: Style.font.display
  }

  Column {
    id: labels
    anchors.left: icon.right
    anchors.leftMargin: Style.space(12)
    anchors.right: parent.right
    anchors.verticalCenter: parent.verticalCenter
    spacing: Style.space(2)

    Text {
      width: parent.width
      text: root.title
      textFormat: Text.PlainText
      color: root.foreground
      font.family: root.fontFamily
      font.pixelSize: Style.font.title
      font.bold: true
      elide: Text.ElideRight
    }
    Text {
      width: parent.width
      text: root.subtitle
      textFormat: Text.PlainText
      color: Qt.darker(root.foreground, 1.45)
      font.family: root.fontFamily
      font.pixelSize: Style.font.bodySmall
      wrapMode: Text.WordWrap
    }
  }
}
