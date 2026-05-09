import QtQuick 2.15

Rectangle {
    id: root
    color: "black"

    Text {
        anchors.centerIn: parent
        text: "Matrix Screensaver\nRun: ~/.local/bin/matrix-screensaver"
        color: "#00ff41"
        font.family: "monospace"
        font.pointSize: 14
        horizontalAlignment: Text.AlignHCenter
    }
}
