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
                sh 'gmake'
            }
        }
        stage('Test') {
            steps {
                sh 'gmake test'
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