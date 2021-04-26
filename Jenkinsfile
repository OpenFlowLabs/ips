pipeline {
    agent {
        node {
            label 'buildserver'
        }
    }

    options {
        buildDiscarder logRotator(
            daysToKeepStr: '1',
            numToKeepStr: '3'
        )
    }

    stages {
        stage('Build') {
            steps {
                sh '''
                    source $HOME/.profile
                    cargo version
                    gmake
                '''
            }
        }
        stage('Test') {
            steps {
                sh '''
                    source $HOME/.profile
                    cargo version
                    gmake test
                '''
            }
        }
        stage('Release') {
            when { tag "v*" }
            steps {
                archiveArtifacts artifacts: 'artifacts/**', fingerprint: true
            }
        }
    }
}